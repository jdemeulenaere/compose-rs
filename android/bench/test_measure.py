#!/usr/bin/env python3
"""measure.py's parsing, on captured output (no device): python3 android/bench/test_measure.py"""

import contextlib
import io
import json
import os
import tempfile
import unittest
import zipfile

import measure
import summarize

# `dumpsys package dexopt` on a Pixel 8 Pro, Android 16 (ART Service), shortened.
DEXOPT_ART_SERVICE = """\
Dexopt state:
  [com.google.android.apps.photos]
    path: /data/app/~~abc==/com.google.android.apps.photos-def==/base.apk
      arm64: [status=speed-profile] [reason=bg-dexopt] [primary-abi]
        [location is /data/app/~~abc==/com.google.android.apps.photos-def==/oat/arm64/base.odex]
  [rs.compose.bench.compose]
    path: /data/app/~~O36TqJP4IvC859L-FshBFw==/rs.compose.bench.compose-Ffr_hYv3EAktjMXJmRouuA==/base.apk
      arm64: [status=speed-profile] [reason=cmdline] [primary-abi]
        [location is /data/app/~~O36TqJP4IvC859L-FshBFw==/rs.compose.bench.compose-Ffr_hYv3EAktjMXJmRouuA==/oat/arm64/base.odex]
  [rs.compose.bench.rs]
    path: /data/app/~~bWaDI3LXU_90ysxQwhPp9g==/rs.compose.bench.rs-Za_P4XFHb7SdniUrRpf0hw==/base.apk
      arm64: [status=verify] [reason=cmdline] [primary-abi]
        [location is /data/app/~~bWaDI3LXU_90ysxQwhPp9g==/rs.compose.bench.rs-Za_P4XFHb7SdniUrRpf0hw==/oat/arm64/base.vdex]

Dexopt dump of secondary dex files:
"""

# The format before ART Service (Android 13 and earlier): one state per instruction set.
DEXOPT_LEGACY = """\
Dexopt state:
  [rs.compose.bench.rs]
    path: /data/app/~~x==/rs.compose.bench.rs-y==/base.apk
      arm: [status=run-from-apk] [reason=unknown]
      arm64: [status=speed] [reason=cmdline]
"""


def write_apk(path, files):
    with zipfile.ZipFile(path, "w") as archive:
        for name, data in files.items():
            archive.writestr(name, data)


class DexoptTest(unittest.TestCase):
    def test_art_service(self):
        states = measure.dexopt_states(DEXOPT_ART_SERVICE)
        self.assertEqual(states["rs.compose.bench.rs"], [{
            "path": "/data/app/~~bWaDI3LXU_90ysxQwhPp9g==/rs.compose.bench.rs-Za_P4XFHb7SdniUrRpf0hw==/base.apk",
            "isa": "arm64", "status": "verify", "reason": "cmdline"}])
        self.assertEqual([(state["status"], state["reason"])
                          for state in states["rs.compose.bench.compose"]],
                         [("speed-profile", "cmdline")])
        self.assertEqual(states["com.google.android.apps.photos"][0]["reason"], "bg-dexopt")

    def test_legacy(self):
        states = measure.dexopt_states(DEXOPT_LEGACY)
        self.assertEqual([(state["isa"], state["status"]) for state in states["rs.compose.bench.rs"]],
                         [("arm", "run-from-apk"), ("arm64", "speed")])

    def test_missing_package(self):
        self.assertNotIn("rs.compose.bench.rs", measure.dexopt_states("Dexopt state:\n"))


class ApkTest(unittest.TestCase):
    def test_apk_paths(self):
        output = ("package:/data/app/~~a==/rs.compose.bench.rs-b==/split_config.arm64_v8a.apk\n"
                  "package:/data/app/~~a==/rs.compose.bench.rs-b==/base.apk\n")
        self.assertEqual(measure.apk_paths(output), [
            "/data/app/~~a==/rs.compose.bench.rs-b==/base.apk",
            "/data/app/~~a==/rs.compose.bench.rs-b==/split_config.arm64_v8a.apk"])
        # `pm path` for a missing package prints nothing on stdout (and exits 1).
        self.assertEqual(measure.apk_paths(""), [])

    def test_device_abi_only(self):
        """A built APK with every ABI against an install of the device's ABI only (Android
        Studio's), or as a base and a split: only lib/arm64-v8a/ is compared."""
        with tempfile.TemporaryDirectory() as directory:
            def path(name):
                return os.path.join(directory, name)
            library = "lib/arm64-v8a/libcompose_rs_feed.so"
            other = "lib/x86_64/libcompose_rs_feed.so"
            write_apk(path("built.apk"), {"classes.dex": b"dex", library: b"new", other: b"x86"})
            write_apk(path("studio.apk"), {"classes.dex": b"dex", library: b"new"})
            write_apk(path("base.apk"), {"classes.dex": b"dex"})
            write_apk(path("split.apk"), {library: b"new"})
            write_apk(path("stale-split.apk"), {library: b"old"})
            built = measure.apk_summary(path("built.apk"))

            studio = measure.installed_summary([path("studio.apk")])
            differ, problems = measure.compare_apks(studio, built, "arm64-v8a")
            self.assertFalse(differ)
            self.assertTrue(problems[0].startswith("APK"), problems)

            split = measure.installed_summary([path("base.apk"), path("split.apk")])
            self.assertEqual(split["splits"], 1)
            self.assertEqual(split["bytes"], os.path.getsize(path("base.apk"))
                             + os.path.getsize(path("split.apk")))
            self.assertEqual(split["sha1"], measure.apk_summary(path("base.apk"))["sha1"])
            self.assertFalse(measure.compare_apks(split, built, "arm64-v8a")[0])

            stale = measure.installed_summary([path("base.apk"), path("stale-split.apk")])
            differ, problems = measure.compare_apks(stale, built, "arm64-v8a")
            self.assertTrue(differ)
            self.assertTrue(problems[0].startswith("arm64-v8a libraries"), problems)

            # No library for the device's ABI in a built APK that has some: wrong too.
            self.assertTrue(measure.compare_apks(studio, built, "armeabi-v7a")[0])

    def test_summary_and_comparison(self):
        with tempfile.TemporaryDirectory() as directory:
            built_path = os.path.join(directory, "built.apk")
            same_path = os.path.join(directory, "same.apk")
            stale_path = os.path.join(directory, "stale.apk")
            dex_path = os.path.join(directory, "dex.apk")
            library = "lib/arm64-v8a/libcompose_rs_feed.so"
            write_apk(built_path, {"classes.dex": b"dex", library: b"new library"})
            write_apk(same_path, {"classes.dex": b"dex", library: b"new library"})
            write_apk(stale_path, {"classes.dex": b"dex", library: b"old library"})
            write_apk(dex_path, {"classes.dex": b"other dex", library: b"new library"})
            built = measure.apk_summary(built_path)
            self.assertEqual(built["bytes"], os.path.getsize(built_path))
            self.assertEqual(list(built["libraries"]), [library])
            self.assertEqual(built["libraries"][library]["bytes"], len(b"new library"))
            # Zip entries carry timestamps: compare the libraries, not the archives.
            self.assertEqual(measure.apk_summary(same_path)["libraries"], built["libraries"])
            differ, stale = measure.compare_apks(
                measure.installed_summary([stale_path]), built, "arm64-v8a")
            self.assertTrue(differ)
            self.assertTrue(stale[0].startswith("arm64-v8a libraries"), stale)
            differ, dex = measure.compare_apks(
                measure.installed_summary([dex_path]), built, "arm64-v8a")
            self.assertFalse(differ)
            self.assertEqual(len(dex), 1)
            self.assertTrue(dex[0].startswith("APK"), dex)
            self.assertEqual(measure.compare_apks(built, None, "arm64-v8a"), (False, []))


class HygieneTest(unittest.TestCase):
    def test_our_other_apps_are_stopped(self):
        output = ("package:rs.compose.bench.rs\n"
                  "package:rs.compose.demo\n"
                  "package:rs.compose.bench.compose\n"
                  "package:com.google.android.apps.photos\n")
        self.assertEqual(measure.packages_to_stop(output, "rs.compose.bench.rs"),
                         ["rs.compose.bench.compose", "rs.compose.demo"])
        self.assertEqual(measure.packages_to_stop("", "rs.compose.bench.rs"), [])

    def test_the_warm_up_is_left_out(self):
        ms = 1_000_000
        warm_up = [1000 * ms + step * 8 * ms for step in range(15)]
        flings = [2500 * ms + step * 8 * ms for step in range(15)]
        flings += [3800 * ms + step * 8 * ms for step in range(15)]
        self.assertEqual(measure.measured_since(flings + warm_up), 2500 * ms)
        # No pause: no warm-up to leave out.
        self.assertEqual(measure.measured_since(warm_up), 0)
        self.assertEqual(measure.measured_since([]), 0)


class BaselineProfileTest(unittest.TestCase):
    # `am broadcast` to androidx.profileinstaller's receiver on a Pixel 8 Pro, Android 16.
    INSTALLED = (
        "Broadcasting: Intent { act=androidx.profileinstaller.action.INSTALL_PROFILE flg=0x400000 "
        "cmp=rs.compose.bench.rs/androidx.profileinstaller.ProfileInstallReceiver }\n"
        "Broadcast completed: result=1\n")

    def test_broadcast_result(self):
        self.assertEqual(measure.broadcast_result(self.INSTALLED), measure.PROFILE_INSTALLED)
        # Another result: the profile wasn't installed (e.g. 6, none in the APK).
        self.assertEqual(measure.broadcast_result("Broadcast completed: result=6\n"), 6)
        # No receiver: an app without profileinstaller.
        self.assertEqual(measure.broadcast_result("Broadcast completed: result=0\n"), 0)
        self.assertIsNone(measure.broadcast_result("Error: something failed\n"))

    def test_an_app_without_its_profile_stops_the_run(self):
        calls = []
        original_shell, original_stop = measure.shell, measure.stop
        try:
            measure.stop = lambda package: calls.append(("stop", package))
            measure.shell = lambda command: "Broadcast completed: result=0\n"
            with self.assertRaises(SystemExit):
                measure.install_baseline_profile("rs.compose.bench.rs")
            measure.shell = lambda command: self.INSTALLED
            measure.install_baseline_profile("rs.compose.bench.rs")
        finally:
            measure.shell, measure.stop = original_shell, original_stop
        # The receiver's process is stopped either way, so the run starts cold.
        self.assertEqual(calls, [("stop", "rs.compose.bench.rs")] * 2)


class CompiledTest(unittest.TestCase):
    def test_each_app_must_be_compiled_as_asked(self):
        states = measure.dexopt_states(DEXOPT_ART_SERVICE)
        apps = {"compose": states["rs.compose.bench.compose"], "rs": states["rs.compose.bench.rs"]}
        # compose-rs's app is at verify: a speed-profile run stops.
        with self.assertRaises(SystemExit):
            measure.check_compiled(apps, "speed-profile")
        measure.check_compiled({"compose": apps["compose"]}, "speed-profile")
        with self.assertRaises(SystemExit):
            measure.check_compiled({"rs": []}, "verify")


class EmptyTraceTest(unittest.TestCase):
    def test_a_run_without_frames_is_run_again(self):
        results = iter([{"trace": {"frames": 0}}, {"trace": {"frames": 1200}}])
        self.assertEqual(measure.traced(lambda: next(results), "animation"), {"trace": {"frames": 1200}})
        results = iter([("memory", {"trace": {"frames": 0}}), ("memory", {"trace": {"frames": 9}})])
        self.assertEqual(measure.traced(lambda: next(results), "scroll")[1]["trace"]["frames"], 9)
        with self.assertRaises(SystemExit):
            measure.traced(lambda: {"trace": {"frames": 0}}, "scroll")


GFXINFO_GPU_CACHES = """
GPU Caches:
  Other:
    Other: 6.56 KB (1 entry)
    Buffer Object: 2.05 KB (2 entries)
  SW Path Mask:
    Surface: 7.98 MB (50 entries)
  Scratch:
    Buffer Object: 27.95 MB (159 entries)
    Surface: 58.63 MB (188 entries)
Total GPU memory usage:
  99163112 bytes, 94.57 MB (30.09 MB is purgeable)
Pipeline Cache: inactive
"""


class GpuCachesTest(unittest.TestCase):
    def test_categories_total_and_purgeable(self):
        original = measure.shell
        try:
            measure.shell = lambda command: GFXINFO_GPU_CACHES
            caches = measure.gpu_caches("rs.compose.bench.rs")
        finally:
            measure.shell = original
        self.assertEqual(caches["Scratch/Surface"], [58.63, 188])
        self.assertEqual(caches["Scratch/Buffer Object"], [27.95, 159])
        self.assertEqual(caches["SW Path Mask/Surface"], [7.98, 50])
        self.assertEqual(caches["Other/Other"], [0.01, 1])
        self.assertEqual((caches["total"], caches["purgeable"]), (94.57, 30.09))


class QueryTest(unittest.TestCase):
    def test_a_failed_query_stops_the_run(self):
        # A trace processor exiting with an error must not read as no rows (zero counts).
        with self.assertRaises(RuntimeError):
            measure.query("false", "trace", "SELECT 1")


class JankTest(unittest.TestCase):
    def test_sides(self):
        side = measure.jank_side
        self.assertIsNone(side("None"))
        self.assertIsNone(side("Buffer Stuffing"))
        self.assertEqual(side("App Deadline Missed"), "app")
        self.assertEqual(side("App Deadline Missed, Buffer Stuffing"), "app")
        self.assertEqual(side("Prediction Error"), "compositor")
        self.assertEqual(side("SurfaceFlinger CPU Deadline Missed"), "compositor")
        self.assertEqual(side("Display HAL, Buffer Stuffing"), "compositor")
        # Both: the app's, as it missed its own deadline.
        self.assertEqual(side("SurfaceFlinger Scheduling, App Deadline Missed"), "app")
        self.assertEqual(side("Unknown"), "other")
        self.assertEqual(side("Dropped Frame"), "other")

    def test_run_line(self):
        trace = {
            "jank_sides": {"app": 1, "compositor": 2, "other": 0},
            "visibly_late_frames": 31, "late_do_frames": 15, "late_do_frames_after_work": 2,
            "prefetches": 69,
            "buffer_stuffing": {"recovery_slices": 1216, "buffer_stuffed": 1, "occurred": True},
            "main_thread_frame_ms": {"p50": 1.1, "p90": 2.5, "p99": 7.4},
            "render_thread_frame_ms": {"p50": 2.1, "p90": 3.4, "p99": 5.2},
        }
        line = summarize.run_line(trace)
        self.assertIn("janky app 1 compositor 2 other 0", line)
        self.assertIn("visibly late 31, late doFrames 15 (after work 2), prefetches 69", line)
        self.assertIn("buffer stuffing yes (1216 recovery slices, 1 stuffed)", line)
        self.assertIn("main p50 1.10 p90 2.50 p99 7.40", line)
        trace["buffer_stuffing"] = {"recovery_slices": 0, "buffer_stuffed": 0, "occurred": False}
        self.assertIn("buffer stuffing no |", summarize.run_line(trace))
        # Reports from before D343 have none of it.
        self.assertIsNone(summarize.run_line({"frames": 3}))


class BoostTest(unittest.TestCase):
    MS = 1_000_000
    # A Pixel 8 Pro's cores (`cpu_capacities()`): 4 little, 4 mid, 1 big.
    PIXEL_8_PRO = {**{cpu: (255, 1704000) for cpu in range(4)},
                   **{cpu: (861, 2367000) for cpu in range(4, 8)}, 8: (1024, 2914000)}

    def frames(self, starts_ms, dur_ms):
        return [(start * self.MS, int(dur * self.MS)) for start, dur in zip(starts_ms, dur_ms)]

    def test_buckets_and_time_weighted_minimum(self):
        ms = self.MS
        # Frames every 10 ms from 100 to 190 ms; the minimum 2 until 120 ms, then 150 until
        # 160 ms, then 300.
        frames = self.frames(range(100, 200, 10), [3, 3, 2, 2, 2, 2, 1, 1, 1, 1])
        samples = [(50 * ms, 2), (120 * ms, 150), (160 * ms, 300)]
        picture = measure.boost_picture(frames, samples, {}, {})
        self.assertTrue(picture["counter"])
        # Over 100 to 191 ms: 20 ms at 2, 40 ms at 150, 31 ms at 300.
        self.assertAlmostEqual(picture["time_weighted_min"], (20 * 2 + 40 * 150 + 31 * 300) / 91)
        self.assertEqual(picture["frames_before_counter"], 0)
        self.assertAlmostEqual(picture["unboosted_frame_share"], 0.2)
        buckets = picture["buckets"]
        self.assertEqual([buckets[name]["frames"] for name in ("unboosted", "100-199", "200+")],
                         [2, 4, 4])
        self.assertAlmostEqual(buckets["100-199"]["share"], 0.4)
        self.assertEqual(buckets["unboosted"]["p50"], 3.0)
        self.assertEqual(buckets["200+"]["p99"], 1.0)
        # Little cores' share is None without capacities.
        self.assertIsNone(picture["little_core_share"])

    def test_bucket_edges(self):
        frames = self.frames([10, 20, 30, 40], [1, 2, 3, 4])
        samples = [(10 * self.MS, 99), (20 * self.MS, 100), (30 * self.MS, 199), (40 * self.MS, 200)]
        buckets = measure.boost_picture(frames, samples, {}, {})["buckets"]
        self.assertEqual([buckets[name]["frames"] for name in ("unboosted", "100-199", "200+")],
                         [1, 2, 1])

    def test_without_the_counter(self):
        frames = self.frames([10, 20], [2, 2])
        picture = measure.boost_picture(frames, [], {0: 3 * self.MS, 8: self.MS}, self.PIXEL_8_PRO)
        self.assertFalse(picture["counter"])
        self.assertIsNone(picture["time_weighted_min"])
        self.assertIsNone(picture["unboosted_frame_share"])
        self.assertIsNone(picture["buckets"])
        # Where it ran is known without the counter.
        self.assertAlmostEqual(picture["little_core_share"], 0.75)
        line = summarize.boost_line({"render_boost": picture})
        self.assertIn("little cores 75% of running", line)
        self.assertIn("no HWUI boost counter", line)

    def test_frames_before_the_first_value_are_left_out(self):
        frames = self.frames([10, 20, 30], [5, 4, 2])
        picture = measure.boost_picture(frames, [(25 * self.MS, 250)], {}, {})
        self.assertEqual(picture["frames_before_counter"], 2)
        self.assertEqual(picture["buckets"]["200+"]["frames"], 1)
        self.assertEqual(picture["unboosted_frame_share"], 0.0)
        # Only the span from the first value counts: 250 throughout.
        self.assertEqual(picture["time_weighted_min"], 250)
        # A counter whose values all come after the frames: nothing bucketed.
        picture = measure.boost_picture(frames, [(90 * self.MS, 250)], {}, {})
        self.assertTrue(picture["counter"])
        self.assertIsNone(picture["buckets"])
        self.assertIn("no frame with a boost value", summarize.boost_line({"render_boost": picture}))

    def test_little_cores_by_capacity(self):
        share = measure.little_core_share
        running = {0: 2, 3: 2, 5: 4, 8: 2}
        self.assertAlmostEqual(share(running, self.PIXEL_8_PRO), 0.4)
        # Another layout: 6 little cores and 2 big ones.
        other = {**{cpu: (400, 1800000) for cpu in range(6)},
                 **{cpu: (1024, 3000000) for cpu in range(6, 8)}}
        self.assertAlmostEqual(share({0: 1, 5: 1, 6: 2}, other), 0.5)
        # Cores all alike have no little ones; no capacities read, or no running time: unknown.
        self.assertIsNone(share(running, {cpu: (1024, 1) for cpu in range(9)}))
        self.assertIsNone(share(running, {}))
        self.assertIsNone(share({}, self.PIXEL_8_PRO))
        # A CPU missing from the capacities counts as running, not as little.
        self.assertAlmostEqual(share({0: 1, 9: 1}, self.PIXEL_8_PRO), 0.5)

    def test_summary_lines(self):
        frames = self.frames(range(100, 200, 10), [3, 3, 2, 2, 2, 2, 1, 1, 1, 1])
        samples = [(50 * self.MS, 2), (120 * self.MS, 150), (160 * self.MS, 300)]
        picture = measure.boost_picture(frames, samples, {0: 1, 4: 3}, self.PIXEL_8_PRO)
        line = summarize.boost_line({"render_boost": picture})
        self.assertIn("little cores 25% of running, boost min time-weighted 169, frames unboosted 20%", line)
        self.assertIn("| unboosted 2 (20%) p50 3.00 p90 3.00 p99 3.00", line)
        self.assertIn("| 200+ 4 (40%) p50 1.00", line)
        unboosted = measure.boost_picture(frames, [(0, 2)], {0: 1}, self.PIXEL_8_PRO)
        self.assertIn("| 200+ 0 (0%) |", summarize.boost_line({"render_boost": unboosted}) + " |")
        no_counter = measure.boost_picture(frames, [], {0: 1}, self.PIXEL_8_PRO)
        traces = [{"render_boost": picture}, {"render_boost": unboosted},
                  {"render_boost": no_counter}, {"frames": 3}]
        summary = summarize.boost_summary(traces)
        # Little cores: 25 %, 100 %, 100 %; the boost's medians of the two runs with the counter.
        self.assertIn("little cores 100% of running", summary)
        self.assertIn("frames unboosted 60%", summary)
        self.assertIn("| unboosted 60% p50 2.50", summary)
        # 200+: shares 40 % and 0 %; percentiles of the run with frames in it.
        self.assertIn("| 200+ 20% p50 1.00", summary)
        # Reports from before round 6 have no boost picture.
        self.assertIsNone(summarize.boost_line({"frames": 3}))
        self.assertIsNone(summarize.boost_summary([{"frames": 3}]))
        self.assertIn("no HWUI boost counter", summarize.boost_summary([{"render_boost": no_counter}]))


class ScenariosTest(unittest.TestCase):
    def test_all_by_default(self):
        self.assertEqual(measure.parse_scenarios("all"), list(measure.SCENARIOS))
        self.assertEqual(measure.parse_scenarios(""), list(measure.SCENARIOS))
        self.assertEqual(measure.parse_scenarios(None), list(measure.SCENARIOS))

    def test_some_in_their_order(self):
        self.assertEqual(measure.parse_scenarios("stress_animation, stress_scroll"),
                         ["stress_scroll", "stress_animation"])
        self.assertEqual(measure.parse_scenarios("scroll,stress_startup"),
                         ["scroll", "stress_startup"])

    def test_an_unknown_name_stops_the_run(self):
        with self.assertRaises(SystemExit):
            measure.parse_scenarios("scroll,stress")


# Rust's table, shortened (crates/compose-ui/src/optimizations.rs).
OPTIMIZATIONS_RS = """\
    pub const SINGLE_DRAWING_FADE: Optimization = Optimization {
        bit: 0,
        name: "single_drawing_fade",
        default: false,
    };
    /// (A doc comment naming no `name: "x"`.)
    pub const PERFORMANCE_HINT: Optimization = Optimization {
        bit: 1,
        name: "performance_hint",
        default: true,
    };
    pub const KEEP_ITEMS_BEHIND: Optimization = Optimization {
        bit: 6,
        name: "keep_items_behind",
        default: false,
    };
"""


class OptimizationsTest(unittest.TestCase):
    NAMES = ["single_drawing_fade", "performance_hint", "keep_items_behind"]

    def test_names_from_rusts_table(self):
        self.assertEqual(measure.optimization_names(OPTIMIZATIONS_RS), self.NAMES)
        # The real table: every name, the hint session's among them.
        with open(measure.OPTIMIZATIONS_SOURCE) as file:
            names = measure.optimization_names(file.read())
        self.assertIn("performance_hint", names)
        self.assertIn("prefetch_two_ahead", names)
        self.assertEqual(len(names), len(set(names)))

    def test_parity_by_default_runs_bench_rs_with_none(self):
        for text in (None, "", "parity"):
            mode, subjects = measure.parse_optimizations(text, self.NAMES)
            self.assertEqual(mode, "parity")
            self.assertEqual(subjects, [("compose", "rs.compose.bench.compose", None),
                                        ("rs", "rs.compose.bench.rs", "none")])
        mode, subjects = measure.parse_optimizations("all", self.NAMES)
        self.assertEqual((mode, subjects[1][2]), ("all", "all"))

    def test_each_runs_bench_rs_at_parity_and_with_each_alone(self):
        mode, subjects = measure.parse_optimizations("each", self.NAMES)
        self.assertEqual(mode, "each")
        self.assertEqual(subjects, [
            ("rs", "rs.compose.bench.rs", "none"),
            ("rs+single_drawing_fade", "rs.compose.bench.rs", "single_drawing_fade"),
            ("rs+performance_hint", "rs.compose.bench.rs", "performance_hint"),
            ("rs+keep_items_behind", "rs.compose.bench.rs", "keep_items_behind")])

    def test_rs_runs_bench_rs_alone_at_parity(self):
        mode, subjects = measure.parse_optimizations("rs", self.NAMES)
        self.assertEqual((mode, subjects), ("rs", [("rs", "rs.compose.bench.rs", "none")]))

    def test_each_only_some(self):
        mode, subjects = measure.parse_optimizations("each", self.NAMES, "keep_items_behind")
        self.assertEqual([name for name, _, _ in subjects], ["rs", "rs+keep_items_behind"])
        with self.assertRaises(SystemExit):
            measure.parse_optimizations("each", self.NAMES, "faster")

    def test_bits_as_rusts_parse(self):
        """Each bit and default from the table's own fields: keep_items_behind is bit 6."""
        table = measure.optimization_table(OPTIMIZATIONS_RS)
        self.assertEqual(table, [("single_drawing_fade", 0, False), ("performance_hint", 1, True),
                                 ("keep_items_behind", 6, False)])
        self.assertEqual(measure.optimization_bits("none", table), 0)
        self.assertEqual(measure.optimization_bits(None, table), 0b10)
        self.assertEqual(measure.optimization_bits("all", table), 0b1000011)
        self.assertEqual(measure.optimization_bits("all,-performance_hint", table), 0b1000001)
        self.assertEqual(measure.optimization_bits("keep_items_behind", table), 1 << 6)
        with open(measure.OPTIMIZATIONS_SOURCE) as file:
            source = file.read()
        # The real table: every row read, the recommended set on by default (D610), so bench-rs
        # without `-e optimizations` runs the recommended set, not parity: measure.py always
        # passes the extra (`test_bench_rs_always_gets_its_list`).
        real = measure.optimization_table(source)
        recommended = measure.optimization_recommended(source)
        self.assertEqual(len(real), source.count("Optimization = Optimization {"))
        self.assertEqual(sorted(measure.optimization_defaults(source)), sorted(recommended))
        self.assertEqual(measure.optimization_bits(None, real),
                         measure.optimization_bits("recommended", real, recommended))
        self.assertEqual(measure.optimization_bits("default", real),
                         measure.optimization_bits(None, real))
        self.assertEqual(measure.optimization_bits("none", real), 0)

    def test_bench_rs_always_gets_its_list(self):
        """Every mode starts bench-rs with an `-e optimizations` (its default is the recommended
        set, D610): parity passes `none`; only the Compose app gets none."""
        cases = [measure.parse_optimizations(text, self.NAMES)
                 for text in (None, "parity", "recommended", "all", "rs", "each",
                              "keep_items_behind")]
        cases.append(measure.parse_subjects(["recommended", "none"], self.NAMES))
        for mode, subjects in cases:
            for name, package, optimizations in subjects:
                if package == "rs.compose.bench.rs":
                    self.assertIsNotNone(optimizations, f"{mode}: {name}")
                else:
                    self.assertIsNone(optimizations, f"{mode}: {name}")
        _, subjects = measure.parse_optimizations("parity", self.NAMES)
        self.assertEqual(subjects[1][2], "none")

    def test_main_wires_the_arguments(self):
        """`main`'s configuration: the subjects rotated, each one's bits, the thermal limits."""
        with open(measure.OPTIMIZATIONS_SOURCE) as file:
            source = file.read()
        names = measure.optimization_names(source)
        parser_arguments = type("Arguments", (), {
            "optimizations": "each", "each_only": "single_drawing_fade,performance_hint",
            "battery_limit": 34.0, "thermal_wait_limit": 0, "skin_limit": 35.0, "rotate": 1})
        original = dict(measure.THERMAL)
        try:
            mode, subjects, bits = measure.configure(parser_arguments, source)
            limits = dict(measure.THERMAL)
        finally:
            measure.THERMAL.update(original)
        self.assertEqual(mode, "each")
        self.assertEqual([name for name, _, _ in subjects],
                         ["rs+single_drawing_fade", "rs+performance_hint", "rs"])
        self.assertEqual(bits, {"rs": 0, "rs+single_drawing_fade": 1 << names.index("single_drawing_fade"),
                                "rs+performance_hint": 1 << names.index("performance_hint")})
        self.assertEqual((limits["battery_limit_c"], limits["wait_limit_s"], limits["skin_limit_c"]),
                         (34.0, 0, 35.0))
        # Parity by default: the Compose app has no bits, bench-rs none.
        parser_arguments.optimizations, parser_arguments.rotate = "parity", 0
        try:
            _, _, bits = measure.configure(parser_arguments, source)
        finally:
            measure.THERMAL.update(original)
        self.assertEqual(bits, {"compose": None, "rs": 0})

    def test_recommended_runs_bench_rs_with_rusts_set(self):
        mode, subjects = measure.parse_optimizations("recommended", self.NAMES)
        self.assertEqual(mode, "recommended")
        self.assertEqual(subjects, [("compose", "rs.compose.bench.compose", None),
                                    ("rs", "rs.compose.bench.rs", "recommended")])
        # In a list too, combined with names.
        mode, subjects = measure.parse_optimizations("recommended,-performance_hint", self.NAMES)
        self.assertEqual((mode, subjects[1][2]), ("list", "recommended,-performance_hint"))

    def test_recommended_bits_from_rusts_table(self):
        source = OPTIMIZATIONS_RS + """\
    pub const RECOMMENDED: &'static [Optimization] = &[
        Optimization::SINGLE_DRAWING_FADE,
        Optimization::KEEP_ITEMS_BEHIND,
    ];
"""
        table = measure.optimization_table(source)
        recommended = measure.optimization_recommended(source)
        self.assertEqual(recommended, ["single_drawing_fade", "keep_items_behind"])
        self.assertEqual(measure.optimization_bits("recommended", table, recommended), 0b1000001)
        self.assertEqual(
            measure.optimization_bits("recommended,performance_hint", table, recommended),
            0b1000011)
        self.assertEqual(
            measure.optimization_bits("recommended,-keep_items_behind", table, recommended), 0b1)
        # A set's name replaces what's before it, as Rust's parse.
        self.assertEqual(
            measure.optimization_bits("performance_hint,recommended", table, recommended),
            0b1000001)
        with self.assertRaises(SystemExit):
            measure.optimization_recommended(OPTIMIZATIONS_RS)
        # The real table: the five flags with a measured gain (D601), as Rust's test says.
        with open(measure.OPTIMIZATIONS_SOURCE) as file:
            real = file.read()
        self.assertEqual(sorted(measure.optimization_recommended(real)), [
            "draw_prefetched_ahead", "flush_layers_before_dropped_paths", "performance_hint",
            "reuse_paragraph_at_same_layout_width", "single_drawing_fade"])

    def test_subjects_run_several_bench_rs_lists_beside_compose(self):
        mode, subjects = measure.parse_subjects(["recommended", "all"], self.NAMES)
        self.assertEqual(mode, "subjects")
        self.assertEqual(subjects, [("compose", "rs.compose.bench.compose", None),
                                    ("rs-recommended", "rs.compose.bench.rs", "recommended"),
                                    ("rs-all", "rs.compose.bench.rs", "all")])
        _, subjects = measure.parse_subjects(
            ["no-hint=recommended, -performance_hint", "none"], self.NAMES)
        self.assertEqual(subjects[1:], [("no-hint", "rs.compose.bench.rs",
                                         "recommended,-performance_hint"),
                                        ("rs-none", "rs.compose.bench.rs", "none")])
        for bad in (["faster"], ["recommended,-performance_hint"], ["all", "all"],
                    ["a b=all"], ["x="], ["compose=all"]):
            with self.assertRaises(SystemExit, msg=bad):
                measure.parse_subjects(bad, self.NAMES)

    def test_main_wires_several_subjects(self):
        """`--subject` subjects, rotated, each one's bits; not with --optimizations."""
        with open(measure.OPTIMIZATIONS_SOURCE) as file:
            source = file.read()
        table = measure.optimization_table(source)
        recommended = measure.optimization_recommended(source)
        arguments = type("Arguments", (), {
            "optimizations": "parity", "each_only": None, "subject": ["recommended", "all"],
            "battery_limit": 34.0, "thermal_wait_limit": 0, "skin_limit": 35.0, "rotate": 1})
        original = dict(measure.THERMAL)
        try:
            mode, subjects, bits = measure.configure(arguments, source)
            arguments.optimizations = "all"
            with self.assertRaises(SystemExit):
                measure.configure(arguments, source)
        finally:
            measure.THERMAL.update(original)
        self.assertEqual(mode, "subjects")
        self.assertEqual([name for name, _, _ in subjects], ["rs-recommended", "rs-all", "compose"])
        every = measure.optimization_bits("all", table)
        self.assertEqual(bits["compose"], None)
        self.assertEqual(bits["rs-all"], every)
        self.assertEqual(bits["rs-recommended"],
                         measure.optimization_bits(",".join(recommended), table))
        self.assertNotEqual(bits["rs-recommended"], every)
        self.assertEqual(bits["rs-recommended"] & ~every, 0)

    def test_the_parser_takes_subjects(self):
        """`--subject` repeated reaches `configure` as a list."""
        captured = {}

        def configure(arguments, source):
            captured["subject"] = arguments.subject
            raise SystemExit(0)

        original = measure.configure
        measure.configure = configure
        try:
            with self.assertRaises(SystemExit):
                measure.main(["--trace-processor", "tp", "--out", "out",
                              "--subject", "recommended", "--subject", "all"])
        finally:
            measure.configure = original
        self.assertEqual(captured["subject"], ["recommended", "all"])

    def test_a_list_is_checked(self):
        mode, subjects = measure.parse_optimizations(
            "all, -performance_hint", self.NAMES)
        self.assertEqual((mode, subjects[1][2]), ("list", "all,-performance_hint"))
        with self.assertRaises(SystemExit):
            measure.parse_optimizations("keep_items_behind,faster", self.NAMES)

    def test_the_rs_app_is_started_with_its_list(self):
        self.assertEqual(
            measure.start_command("rs.compose.bench.rs", "stress", "-W ", "none"),
            "am start -W -n rs.compose.bench.rs/.MainActivity -e screen stress "
            "-e optimizations none")
        # The Compose app gets none.
        self.assertEqual(
            measure.start_command("rs.compose.bench.compose", "feed"),
            "am start -n rs.compose.bench.compose/.MainActivity -e screen feed")


# `dumpsys thermalservice` and `dumpsys battery` on a Pixel 8 Pro, Android 16, shortened.
THERMALSERVICE = """\
IsStatusOverride: false
ThermalEventListeners:
\tcallbacks: 9
\tkilled: false
Thermal Status: 2
Cached temperatures:
\tTemperature{mValue=62.000004, mType=0, mName=LITTLE, mStatus=0}
\tTemperature{mValue=24.900002, mType=2, mName=battery, mStatus=0}
\tTemperature{mValue=37.06034, mType=3, mName=VIRTUAL-SKIN, mStatus=0}
HAL Ready: true
Current temperatures from HAL:
\tTemperature{mValue=38.2, mType=3, mName=VIRTUAL-SKIN, mStatus=0}
"""
BATTERY = """\
Current Battery Service state:
  AC powered: false
  USB powered: true
  level: 100
  temperature: 299
  technology: Li-ion
"""


class ThermalTest(unittest.TestCase):
    def test_status_and_temperatures(self):
        self.assertEqual(measure.thermal_status(THERMALSERVICE), 2)
        self.assertEqual(measure.battery_celsius(BATTERY), 29.9)
        # The HAL's current value, not the cached one.
        self.assertEqual(measure.skin_celsius(THERMALSERVICE), 38.2)
        self.assertEqual(measure.skin_celsius(THERMALSERVICE.split("Current")[0]), 37.1)
        self.assertIsNone(measure.thermal_status(""))
        self.assertIsNone(measure.battery_celsius(""))
        self.assertIsNone(measure.skin_celsius(""))

    def test_a_device_without_a_skin_sensor(self):
        """Another device's skin sensor has its own name; none at all reads as None, and a skin
        limit set then is said not to hold."""
        other = "Thermal Status: 0\n\tTemperature{mValue=33.4, mType=3, mName=skin-therm, mStatus=0}\n"
        self.assertEqual(measure.skin_celsius(other), 33.4)
        none = "Thermal Status: 0\n\tTemperature{mValue=40.0, mType=0, mName=CPU, mStatus=0}\n"
        self.assertIsNone(measure.skin_celsius(none))
        outputs = {"thermalservice": none, "battery": BATTERY}
        original = dict(measure.THERMAL)
        measure.MISSING_SENSORS.clear()
        try:
            measure.THERMAL["skin_limit_c"] = 35.0
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                state = measure.thermal_state(outputs.get)
                measure.thermal_state(outputs.get)
        finally:
            measure.THERMAL.update(original)
            measure.MISSING_SENSORS.clear()
        self.assertEqual(state, {"status": 0, "battery_c": 29.9, "skin_c": None})
        # Said once.
        self.assertEqual(output.getvalue().count("no skin_c"), 1)
        self.assertIn("can't hold", output.getvalue())

    def test_too_warm(self):
        self.assertFalse(measure.too_warm({"status": 0, "battery_c": 35.9}, 36.0))
        self.assertTrue(measure.too_warm({"status": 1, "battery_c": 30.0}, 36.0))
        self.assertTrue(measure.too_warm({"status": 0, "battery_c": 36.1}, 36.0))
        # The skin, with a limit.
        self.assertTrue(measure.too_warm({"status": 0, "battery_c": 33.0, "skin_c": 37.6}, 34.0, 35.0))
        self.assertFalse(measure.too_warm({"status": 0, "battery_c": 33.0, "skin_c": 37.6}, 34.0))
        # Unknown: not too warm.
        self.assertFalse(measure.too_warm({"status": None, "battery_c": None}, 36.0))

    def test_a_run_waits_until_cool_polling_every_ten_seconds(self):
        states = iter([{"status": 1, "battery_c": 37.0}, {"status": 0, "battery_c": 36.5},
                       {"status": 0, "battery_c": 35.5}])
        slept = []
        with contextlib.redirect_stdout(io.StringIO()):
            thermal = measure.wait_until_cool("run", read=lambda: next(states),
                                              sleep=slept.append, battery_limit_c=36.0,
                                              limit_s=600)
        self.assertEqual(slept, [10, 10])
        self.assertEqual(thermal, {"before": {"status": 0, "battery_c": 35.5}, "waited_s": 20,
                                   "gave_up": False})

    def test_a_run_gives_up_waiting_after_the_limit_and_says_so(self):
        slept = []
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            thermal = measure.wait_until_cool("stress-scroll rs 1",
                                              read=lambda: {"status": 0, "battery_c": 40.0},
                                              sleep=slept.append, battery_limit_c=36.0,
                                              limit_s=30)
        self.assertEqual(slept, [10, 10, 10])
        self.assertEqual((thermal["waited_s"], thermal["gave_up"]), (30, True))
        self.assertIn("still too warm after 30 s", output.getvalue())

    def test_no_limit_waits_until_cool(self):
        states = iter([{"status": 0, "battery_c": 39.0 - step * 0.1} for step in range(60)])
        slept = []
        with contextlib.redirect_stdout(io.StringIO()):
            thermal = measure.wait_until_cool("run", read=lambda: next(states),
                                              sleep=slept.append, battery_limit_c=34.0, limit_s=0)
        self.assertEqual((thermal["waited_s"], thermal["gave_up"]), (500, False))
        self.assertLessEqual(thermal["before"]["battery_c"], 34.0)

    def test_our_apps_stop_before_the_wait(self):
        """The last run's app would go on animating while the device cools."""
        calls = []
        original_stop, original_wait = measure.stop, measure.wait_until_cool
        try:
            measure.stop = lambda package: calls.append(("stop", package))
            measure.wait_until_cool = lambda what: calls.append(("wait", what)) or {}
            measure.cool_down("stress-animation rs 0")
        finally:
            measure.stop, measure.wait_until_cool = original_stop, original_wait
        self.assertEqual(calls, [("stop", "rs.compose.bench.compose"), ("stop", "rs.compose.bench.rs"),
                                 ("wait", "stress-animation rs 0")])

    def test_a_cool_device_waits_for_nothing(self):
        thermal = measure.wait_until_cool("run", read=lambda: {"status": 0, "battery_c": 30.0},
                                          sleep=lambda _: self.fail("slept"),
                                          battery_limit_c=36.0, limit_s=600)
        self.assertEqual((thermal["waited_s"], thermal["gave_up"]), (0, False))


class SlowestFramesTest(unittest.TestCase):
    def test_slowest_line(self):
        trace = {"slowest_frames": {
            "main": [{"ts": 1, "ms": 9.5, "slices": [
                {"name": "traversal", "ms": 9.1, "depth": 1},
                {"name": "compose-rs: frame", "ms": 8.7, "depth": 2},
                {"name": "compose-rs: layout", "ms": 6.2, "depth": 3},
                {"name": "compose-rs: text layout", "ms": 3.0, "depth": 4}]}],
            "render": []}}
        self.assertEqual(summarize.slowest_line(trace, "main"),
                         "slowest main frame 9.50 ms: traversal 9.10, compose-rs: frame 8.70, "
                         "compose-rs: layout 6.20")
        # No frame on that thread, or a report from before docs/15.
        self.assertIsNone(summarize.slowest_line(trace, "render"))
        self.assertIsNone(summarize.slowest_line({"frames": 3}, "main"))


# `dumpsys gfxinfo rs.compose.bench.rs framestats` on a Pixel 8 Pro, Android 16, shortened: a
# frame before the tap, the heavy one, one after it, and a flagged one.
FRAMESTATS = """\
Window: rs.compose.bench.rs/rs.compose.bench.rs.MainActivity
---PROFILEDATA---
Flags,FrameTimelineVsyncId,IntendedVsync,Vsync,InputEventId,HandleInputStart,AnimationStart,PerformTraversalsStart,DrawStart,FrameDeadline,FrameInterval,FrameStartTime,SyncQueued,SyncStart,IssueDrawCommandsStart,SwapBuffers,FrameCompleted,DequeueBufferDuration,QueueBufferDuration,GpuCompleted,SwapBuffersCompleted,DisplayPresentTime,CommandSubmissionCompleted,
0,100,1000000000,1000000000,0,1000500000,1000600000,1000700000,1000800000,1016600000,8333333,1000400000,1001000000,1001100000,1001200000,1002000000,1002100000,10000,20000,1003000000,1002200000,0,1002000000,
0,200,2000000000,2000000000,0,2000500000,2000600000,2030700000,2040800000,2016600000,8333333,2000400000,2050000000,2050100000,2051000000,2060000000,2061000000,10000,20000,2065000000,2061500000,2070000000,2060000000,
0,201,2066000000,2066000000,0,2066500000,2066600000,2066700000,2066800000,2082600000,8333333,2066400000,2067000000,2067100000,2067200000,2068000000,2068100000,10000,20000,2069000000,2068200000,0,2068000000,
1,202,2100000000,2100000000,0,2100500000,2100600000,2190700000,2190800000,2116600000,8333333,2100400000,2191000000,2191100000,2191200000,2192000000,2199000000,10000,20000,2199500000,2199200000,0,2192000000,
---PROFILEDATA---

View hierarchy:
"""


class NavigationTest(unittest.TestCase):
    def test_the_tap_is_the_screens_center(self):
        self.assertEqual(measure.screen_center("Physical size: 1008x2244\n"), (504, 1122))
        self.assertEqual(measure.screen_center("Physical size: 1344x2992\nOverride size: 1008x2244\n"),
                         (504, 1122))
        self.assertIsNone(measure.screen_center("error"))

    def test_framestats_heavy_frame(self):
        frames = measure.framestats_frames(FRAMESTATS)
        self.assertEqual([frame["FrameTimelineVsyncId"] for frame in frames], [100, 200, 201, 202])
        self.assertEqual(measure.latest_intended_vsync(frames[:1]), 1000000000)
        heavy = measure.framestats_heavy_frame(frames, after=1000000000)
        # The flagged frame (202) is slower but left out.
        self.assertEqual(heavy["vsync_id"], 200)
        self.assertEqual((heavy["frame_ms"], heavy["start_delay_ms"], heavy["animation_ms"],
                          heavy["traversal_ms"], heavy["draw_ms"], heavy["sync_ms"],
                          heavy["render_ms"], heavy["gpu_completed_ms"], heavy["present_ms"]),
                         (61.0, 0.6, 30.1, 10.1, 9.2, 0.9, 10.0, 65.0, 70.0))
        # The display's present unknown (0): None, not a negative time.
        self.assertIsNone(measure.framestats_heavy_frame(frames[:1])["present_ms"])
        self.assertIsNone(measure.framestats_heavy_frame(frames, after=2200000000))
        self.assertEqual(measure.framestats_frames("no data"), [])

    def test_the_inputs_own_time_on_the_traces_clock(self):
        name = "deliverInputEvent src=0x1002 eventTimeNano=126142742000000 id=0xd6b178e4"
        self.assertEqual(measure.event_time_ns(name), 126142742000000)
        self.assertIsNone(measure.event_time_ns("deliverInputEvent"))
        # clock_snapshot's rows, as `query` returns them (strings).
        snapshots = [{"snapshot_id": "0", "clock_name": "BOOTTIME", "clock_value": "126313374358256"},
                     {"snapshot_id": "0", "clock_name": "REALTIME", "clock_value": "1792746297049504407"},
                     {"snapshot_id": "0", "clock_name": "MONOTONIC", "clock_value": "126141199924139"}]
        self.assertEqual(measure.monotonic_offset(snapshots), 172174434117)
        self.assertIsNone(measure.monotonic_offset(snapshots[:2]))

    def test_frame_phases_sum_to_the_frame(self):
        # Compose's: a recomposition inside a measure pass counts as composition, the layout's
        # own time as layout, the frame's and traversal's own as other.
        slices = [(2, 1, "animation", 30_000_000),
                  (3, 2, "AndroidOwner:measureAndLayout", 25_000_000),
                  (4, 3, "Compose:recompose", 15_000_000),
                  (5, 4, "SomethingInside", 2_000_000),
                  (6, 1, "traversal", 15_000_000),
                  (7, 6, "Record View#draw()", 12_000_000),
                  (8, 7, "AndroidOwner:draw", 10_000_000),
                  (9, 6, "postAndWait", 2_000_000)]
        phases = measure.frame_phases(50_000_000, slices)
        self.assertEqual(phases, {"other": 5.0 + 1.0 + 5.0, "layout": 10.0, "composition": 15.0,
                                  "record": 2.0, "draw": 10.0, "sync": 2.0})
        self.assertAlmostEqual(sum(phases.values()), 50.0)

    def test_overlap_and_cores(self):
        rows = [(0, 10_000_000, "Running", 0), (10_000_000, 5_000_000, "R", None),
                (15_000_000, 10_000_000, "Running", 8)]
        self.assertEqual(measure.overlap_by(rows, 5_000_000, 20_000_000, lambda row: row[2]),
                         {"Running": 10.0, "R": 5.0})
        capacities = {0: (160, 1700000), 4: (450, 2400000), 8: (1024, 3000000)}
        self.assertEqual([measure.core_kind(cpu, capacities) for cpu in (0, 4, 8, 9)],
                         ["little", "mid", "big", "unknown"])

    def records(self, offset=None):
        ms = 1_000_000
        markers = [105 * ms, 2105 * ms, 4300 * ms]
        # The second navigation's marker is in a doFrame; the third's in none (not counted).
        frames = [(1, 104 * ms, 40 * ms, 10), (2, 150 * ms, 2 * ms, 11), (3, 2104 * ms, 60 * ms, 20)]
        inputs = [(90 * ms, "deliverInputEvent src=0x1002 eventTimeNano=89000000 id=0x1"),
                  (91 * ms, "deliverInputEvent src=0x1002 eventTimeNano=90000000 id=0x2"),
                  (2100 * ms, "deliverInputEvent src=0x1002 eventTimeNano=2099000000 id=0x3")]
        render = [(145 * ms, 10 * ms, 10), (2165 * ms, 20 * ms, 20)]
        gpu = [(156 * ms, 4 * ms), (2186 * ms, 6 * ms)]
        timeline = {10: (104 * ms, 66 * ms, "App Deadline Missed", "Late Present"),
                    20: (2104 * ms, 96 * ms, "None", "On-time Present")}
        expected = {10: (100 * ms, 16 * ms, "", ""), 20: (2100 * ms, 16 * ms, "", "")}
        return measure.navigation_records(markers, frames, inputs, render, gpu, timeline,
                                          expected, offset)

    def test_each_navigations_first_frame(self):
        first, second = self.records()
        # From the tap's up (the last input delivered before the frame).
        self.assertEqual(first["input"], "delivery")
        self.assertEqual((first["vsync_id"], first["frame_start_ms"], first["main_end_ms"],
                          first["render_end_ms"], first["gpu_end_ms"],
                          first["input_to_present_ms"], first["do_frame_ms"],
                          first["render_ms"], first["gpu_wait_ms"], first["late_ms"]),
                         (10, 13.0, 53.0, 64.0, 69.0, 79.0, 40.0, 10.0, 4.0, 54.0))
        self.assertEqual((first["jank_type"], first["present_type"]),
                         ("App Deadline Missed", "Late Present"))
        self.assertEqual((second["vsync_id"], second["input_to_present_ms"]), (20, 100.0))
        # With the clocks' offset, from the event's own time: 1 ms before its delivery.
        first, second = self.records(offset=0)
        self.assertEqual((first["input"], first["delivery_ms"], first["input_to_present_ms"]),
                         ("event_time", 1.0, 80.0))
        # An offset placing the event after its delivery is wrong: the delivery is used.
        self.assertEqual(self.records(offset=5_000_000)[0]["input"], "delivery")

    def test_the_summary(self):
        records = self.records()
        for record in records:
            record["phases_ms"] = {"composition": 10.0, "layout": 20.0}
        records[1]["phases_ms"]["draw"] = 4.0
        heavy = measure.framestats_heavy_frame(measure.framestats_frames(FRAMESTATS))
        traced = [{"trace": {"navigations": records}, "framestats": [heavy, None]}]
        untraced = [{"framestats": [heavy, heavy]}]
        summary = measure.navigation_summary(traced, untraced)
        self.assertEqual(summary["traced"]["input_to_present_ms"],
                         {"count": 2, "p50": 79.0, "p90": 100.0, "max": 100.0})
        self.assertEqual(summary["traced"]["do_frame_ms"]["max"], 60.0)
        self.assertEqual(summary["phases_ms"]["draw"]["count"], 2)
        self.assertEqual(summary["phases_ms"]["draw"]["max"], 4.0)
        self.assertEqual(summary["inputs"], {"event_time": 0, "delivery": 2})
        self.assertEqual(summary["janky"], 1)
        self.assertEqual(summary["framestats_untraced"]["frame_ms"]["count"], 2)
        self.assertEqual(summary["framestats_all"]["frame_ms"]["count"], 3)

    def test_navigation_runs_only_when_named(self):
        self.assertNotIn("navigation", measure.parse_scenarios("all"))
        self.assertEqual(measure.parse_scenarios("navigation"), ["navigation"])
        self.assertEqual(measure.parse_scenarios("navigation,startup"), ["startup", "navigation"])

    def test_summarize_prints_the_navigations(self):
        records = self.records()
        for record in records:
            record["phases_ms"] = {"composition": 10.0, "layout": 20.0}
        heavy = measure.framestats_heavy_frame(measure.framestats_frames(FRAMESTATS))
        subject = {"traced": [{"trace": {"navigations": records}, "framestats": [heavy]}],
                   "untraced": [{"framestats": [heavy]}]}
        subject["summary"] = measure.navigation_summary(subject["traced"], subject["untraced"])
        report = {"device": "Pixel 8 Pro", "android": "16", "scenarios": ["navigation"],
                  "navigation": {"tap": [504, 1122], "navigations_per_run": 2,
                                 "subjects": {"rs": subject}}}
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "report.json")
            with open(path, "w") as file:
                json.dump(report, file)
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                summarize.main(path)
        text = output.getvalue()
        self.assertIn("navigation rs: input to present p50 79.0 p90 100.0 max 100.0 ms (2)", text)
        self.assertIn("navigation rs phases p50: composition 10.0, layout 20.0", text)
        self.assertIn("navigation rs framestats (untraced): frame p50 61.0 p90 61.0 max 61.0 ms (1)",
                      text)


class SummarizeTest(unittest.TestCase):
    def test_a_report_of_the_stress_scenarios_only(self):
        """A report of `--scenarios stress_scroll,stress_animation` has none of the feed's
        sections: summarize.py prints what's there."""
        def run(memory=False):
            trace = {
                "frames": 100, "janky_frames": 0, "jank_kinds": {},
                "jank_sides": {"app": 0, "compositor": 0, "other": 0},
                "visibly_late_frames": 0, "late_do_frames": 1, "late_do_frames_after_work": 0,
                "prefetches": 0,
                "buffer_stuffing": {"recovery_slices": 0, "buffer_stuffed": 0, "occurred": False},
                "main_thread_frame_ms": {"p50": 1.0, "p90": 2.0, "p99": 3.0, "max": 4.0},
                "render_thread_frame_ms": {"p50": 2.0, "p90": 3.0, "p99": 4.0},
                "cpu_ms": {"main": 100.0, "render": 200.0},
                "work_ms": {"main": 10.0, "render": 20.0},
                "slowest_frames": {"main": [], "render": []}}
            result = {"gfxinfo": {"frames": 100, "p50": 5, "p90": 6, "p99": 8}, "trace": trace}
            if memory:
                result["memory_kb"] = {"after_start": {"Total PSS": 40960},
                                       "after_animation": {"Total PSS": 51200}}
            return result
        report = {
            "device": "Pixel 8 Pro", "android": "16",
            "scenarios": ["stress_scroll", "stress_animation"],
            "stress_memory_kb": {app: {"after_start": [{"Total PSS": 30720}],
                                       "after_scroll": [{"Total PSS": 61440}]}
                                 for app in ("compose", "rs")},
            "stress_scroll": {app: [run()] for app in ("compose", "rs")},
            "stress_animation": {app: [run(memory=True)] for app in ("compose", "rs")},
        }
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "report.json")
            with open(path, "w") as file:
                json.dump(report, file)
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                summarize.main(path)
        text = output.getvalue()
        self.assertIn("scenarios stress_scroll, stress_animation", text)
        self.assertIn("memory stress rs after_scroll: Total PSS 60.0MB", text)
        self.assertIn("stress_scroll rs: gfx frames 100", text)
        self.assertIn("memory stress_animation compose after_animation: Total PSS 50.0MB", text)
        self.assertNotIn("startup", text)

    def test_a_report_of_each_optimization(self):
        """`--optimizations each`: bench-rs's subjects only, with their lists and the thermal
        guard's record of each run."""
        run = {"gfxinfo": {"frames": 100, "p50": 5, "p90": 6, "p99": 8}, "trace": {
            "frames": 100, "janky_frames": 0, "jank_kinds": {},
            "jank_sides": {"app": 0, "compositor": 0, "other": 0},
            "visibly_late_frames": 0, "late_do_frames": 1, "late_do_frames_after_work": 0,
            "prefetches": 0,
            "buffer_stuffing": {"recovery_slices": 0, "buffer_stuffed": 0, "occurred": False},
            "main_thread_frame_ms": {"p50": 1.0, "p90": 2.0, "p99": 3.0, "max": 4.0},
            "render_thread_frame_ms": {"p50": 2.0, "p90": 3.0, "p99": 4.0},
            "cpu_ms": {"main": 100.0, "render": 200.0},
            "work_ms": {"main": 10.0, "render": 20.0},
            "slowest_frames": {"main": [], "render": []}},
            "thermal": {"before": {"status": 0, "battery_c": 31.0, "skin_c": 33.0},
                        "after": {"status": 0, "battery_c": 32.5, "skin_c": 35.0},
                        "waited_s": 20, "gave_up": False}}
        report = {
            "device": "Pixel 8 Pro", "android": "16", "scenarios": ["scroll"],
            "optimizations": {"mode": "each", "subjects": {"rs": "none",
                                                           "rs+keep_items_behind": "keep_items_behind"}},
            "memory_kb": {name: {"after_start": [{"Total PSS": 30720}]}
                          for name in ("rs", "rs+keep_items_behind")},
            "scroll": {name: [run] for name in ("rs", "rs+keep_items_behind")},
        }
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "report.json")
            with open(path, "w") as file:
                json.dump(report, file)
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                summarize.main(path)
        text = output.getvalue()
        self.assertIn("optimizations each rs: none, rs+keep_items_behind: keep_items_behind", text)
        self.assertIn("scroll rs+keep_items_behind: gfx frames 100", text)
        self.assertIn("thermal before: status 0, battery 31.0 °C, skin 33.0 °C; after: status 0, "
                      "battery 32.5 °C, skin 35.0 °C; waited 20 s", text)
        self.assertNotIn("scroll compose", text)


if __name__ == "__main__":
    unittest.main()
