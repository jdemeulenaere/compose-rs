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
        # Android 17's: a frame of an app not animating, which FrameTimeline tags "Non-perceivable
        # Jank" (presented "Unspecified Present"): not jank, as buffer stuffing.
        self.assertIsNone(side("Non Animating"))
        self.assertEqual(side("Non Animating, App Deadline Missed"), "app")

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
        # Compose 1.12.1's view traversal: a Scaffold subcomposing in its measure pass, a text
        # node measuring its paragraph there.
        slices = [(2, 1, "traversal", 40_000_000), (3, 2, "measure", 40_000_000),
                  (4, 3, "AndroidOwner:onMeasure", 39_000_000),
                  (5, 4, "Compose:recompose", 20_000_000),
                  (6, 4, "TextStringSimpleNode::measure", 9_000_000),
                  (7, 6, "TextLayout:initLayout", 7_000_000)]
        self.assertEqual(measure.frame_phases(40_000_000, slices),
                         {"other": 0.0, "layout": 11.0, "composition": 20.0, "text_layout": 9.0})

    def test_overlap_and_cores(self):
        rows = [(0, 10_000_000, "Running", 0), (10_000_000, 5_000_000, "R", None),
                (15_000_000, 10_000_000, "Running", 8)]
        self.assertEqual(measure.overlap_by(rows, 5_000_000, 20_000_000, lambda row: row[2]),
                         {"Running": 10.0, "R": 5.0})
        capacities = {0: (160, 1700000), 4: (450, 2400000), 8: (1024, 3000000)}
        self.assertEqual([measure.core_kind(cpu, capacities) for cpu in (0, 4, 8, 9)],
                         ["little", "mid", "big", "unknown"])
        # Work: 10 ms on the big core at its top frequency, then 5 ms on the little core at
        # half its top (a capacity of 160/1024), 2 ms of which outside the window.
        frequencies = {8: [(0, 30_000_000, 3000000.0)],
                       0: [(0, 20_000_000, 1700000.0), (20_000_000, 10_000_000, 850000.0)]}
        running = [(0, 10_000_000, 8), (20_000_000, 5_000_000, 0)]
        self.assertAlmostEqual(measure.window_work_ms(running, frequencies, capacities, 0, 23_000_000),
                               10.0 + 3.0 * 0.5 * 160 / 1024, places=3)
        self.assertIsNone(measure.window_work_ms(running, {}, capacities, 0, 1))

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
        # A frame whose GPU wait wasn't traced takes none, not the next frame's.
        ms = 1_000_000
        records = measure.navigation_records(
            [105 * ms], [(1, 104 * ms, 40 * ms, 10)], [(91 * ms, "deliverInputEvent")],
            [(145 * ms, 10 * ms, 10), (300 * ms, 5 * ms, 11)], [(301 * ms, 2 * ms)], {}, {})
        self.assertIsNone(records[0]["gpu_end_ms"])
        self.assertIsNone(first["down_to_present_ms"])
        # With the clocks' offset, also from the event's own time (`input tap`'s down's), 1 ms
        # before its delivery; the latency stays from the delivery.
        first, second = self.records(offset=0)
        self.assertEqual((first["input"], first["down_to_delivery_ms"],
                          first["down_to_present_ms"], first["input_to_present_ms"]),
                         ("event_time", 1.0, 80.0, 79.0))
        # An offset placing the event after its delivery is wrong: left out.
        self.assertEqual(self.records(offset=5_000_000)[0]["input"], "delivery")

    def test_the_summary(self):
        records = self.records()
        for record in records:
            record["phases_ms"] = {"composition": 10.0, "layout": 20.0}
        records[1]["phases_ms"]["draw"] = 4.0
        records[0]["render_cores_ms"] = {"mid": 4.0, "big": 1.0}
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
        # The RenderThread's cores, of the navigations whose DrawFrames was found.
        self.assertEqual(summary["render_cores_ms"]["mid"], {"count": 1, "p50": 4.0, "p90": 4.0,
                                                             "max": 4.0})
        self.assertEqual(summary["render_cores_ms"]["little"]["p50"], 0.0)
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


class Android17Test(unittest.TestCase):
    def test_the_trace_holds_the_power_hals_counters(self):
        """Android 17's power HAL traces its ADPF counters (`adpf.<pid>-…-HWUI-…-min`, the
        RenderThread's boost) under atrace's `power` category only."""
        config = measure.TRACE_CONFIG.format(app="rs.compose.bench.rs", duration=1000,
                                             buffer_kb=1024)
        self.assertIn('atrace_categories: "power"', config)

    def test_target_sdk(self):
        output = ("Packages:\n  Package [rs.compose.bench.rs] (a1b2c3):\n    versionCode=1 "
                  "minSdk=29 targetSdk=37\n    versionName=0.1\n")
        self.assertEqual(measure.target_sdk(output), 37)
        self.assertIsNone(measure.target_sdk("Unable to find package: x\n"))


# lock_clocks.sh's state file, after `lock`.
LOCK_STATE = """\
# lock_clocks.sh originals: name min max
original:policy0 324000 1704000
original:policy4 402000 2367000
original:policy8 500000 2914000
original:gpu 150000 890000
original:mif 546000 3744000
locked:policy0 1425000
touched:policy0
locked:policy4 1572000
touched:policy4
locked:policy8 1745000
touched:policy8
locked:gpu 521000
touched:gpu
"""

# `read_clocks`' shell output on a Pixel 8 Pro, locked.
CLOCKS_OUTPUT = """\
policy0 1425000 1425000 1425000 0 1 2 3
policy4 1572000 1572000 1572000 4 5 6 7
policy8 1745000 1745000 1745000 8
gpu 521000 521000 521000
## dsu
 1800000     100
  610000     2000
## mif
 845000     300
 1352000     40
"""


class TraceBufferTest(unittest.TestCase):
    def test_a_wrapped_trace_buffer_is_said(self):
        self.assertEqual(measure.buffer_overwritten(
            [{"name": "traced_buf_bytes_overwritten", "value": "294912"}]), 294912)
        self.assertEqual(measure.buffer_overwritten([]), 0)
        self.assertTrue(issubclass(measure.TraceWrapped, SystemExit))


class LockedClocksTest(unittest.TestCase):
    def test_the_lock_state(self):
        self.assertEqual(measure.locked_clocks(LOCK_STATE),
                         {"policy0": 1425000, "policy4": 1572000, "policy8": 1745000,
                          "gpu": 521000})
        with self.assertRaises(SystemExit):
            measure.locked_clocks("original:policy0 324000 1704000\n")

    def test_the_read_back(self):
        clocks = measure.parse_clocks(CLOCKS_OUTPUT)
        self.assertEqual(clocks["policy4"], {"min": 1572000, "max": 1572000, "cur": 1572000,
                                             "cpus": [4, 5, 6, 7]})
        self.assertEqual(clocks["gpu"], {"min": 521000, "max": 521000, "cur": 521000})
        self.assertEqual(clocks["residency"], {"dsu": {1800000: 100, 610000: 2000},
                                               "mif": {845000: 300, 1352000: 40}})
        expected = measure.locked_clocks(LOCK_STATE)
        self.assertEqual(measure.clocks_moved(expected, clocks), [])

    def test_a_moved_clock_is_said(self):
        expected = measure.locked_clocks(LOCK_STATE)
        moved = measure.parse_clocks(CLOCKS_OUTPUT.replace(
            "policy8 1745000 1745000 1745000", "policy8 1745000 2914000 2914000"))
        self.assertEqual(measure.clocks_moved(expected, moved),
                         ["policy8: min 1745000, max 2914000, now 2914000 (locked at 1745000)"])
        # A CPU capped below its lock (the thermal HAL's own request) is said too.
        capped = measure.parse_clocks(CLOCKS_OUTPUT.replace(
            "policy0 1425000 1425000 1425000", "policy0 1425000 1425000 1197000"))
        self.assertEqual(measure.clocks_moved(expected, capped),
                         ["policy0: min 1425000, max 1425000, now 1197000 (locked at 1425000)"])
        # The GPU's current clock isn't checked: it reads its lowest step when powered off.
        idle = measure.parse_clocks(CLOCKS_OUTPUT.replace("gpu 521000 521000 521000",
                                                          "gpu 521000 521000 150000"))
        self.assertEqual(measure.clocks_moved(expected, idle), [])
        self.assertEqual(measure.clocks_moved(expected, measure.parse_clocks("")),
                         ["policy0: not read", "policy4: not read", "policy8: not read",
                          "gpu: not read"])

    def test_check_clocks_stops_the_run_when_they_moved(self):
        original_read, original_locked = measure.read_clocks, dict(measure.LOCKED)
        try:
            measure.LOCKED.update(expected=measure.locked_clocks(LOCK_STATE))
            measure.read_clocks = lambda: measure.parse_clocks(CLOCKS_OUTPUT)
            self.assertEqual(measure.check_clocks("scroll rs 0")["policy0"]["cur"], 1425000)
            measure.read_clocks = lambda: measure.parse_clocks(CLOCKS_OUTPUT.replace(
                "gpu 521000 521000", "gpu 150000 890000"))
            with self.assertRaises(measure.ClocksMoved):
                measure.check_clocks("scroll rs 0")
            # Unlocked (no --locked-clocks), nothing is read.
            measure.LOCKED.update(expected=None)
            measure.read_clocks = lambda: self.fail("read")
            self.assertIsNone(measure.check_clocks("scroll rs 0"))
        finally:
            measure.read_clocks = original_read
            measure.LOCKED.clear()
            measure.LOCKED.update(original_locked)

    def test_residency_between_two_reads(self):
        before = {"dsu": {610000: 2000, 1800000: 100}, "mif": {845000: 300}}
        after = {"dsu": {610000: 2600, 1800000: 100, 970000: 200}, "mif": {845000: 1300}}
        self.assertEqual(measure.residency_delta(before, after),
                         {"dsu": {"ms": {610000: 600, 970000: 200}, "mean_mhz": 700.0},
                          "mif": {"ms": {845000: 1000}, "mean_mhz": 845.0}})

    def test_the_traces_frequencies_must_be_the_locked_ones(self):
        expected = measure.locked_clocks(LOCK_STATE)
        cpus = {"policy0": [0, 1, 2, 3], "policy4": [4, 5, 6, 7], "policy8": [8]}
        self.assertEqual(measure.trace_clocks_moved(
            expected, cpus, [(0, 1425000, 1425000), (8, 1745000, 1745000)],
            [(1, 0, 521000)]), [])
        self.assertEqual(measure.trace_clocks_moved(
            expected, cpus, [(4, 1572000, 2367000)], [(1, 0, 890000), (0, 0, 900000)]),
            ["cpu 4 at 1572000 to 2367000 in the trace (locked at 1572000)",
             "the GPU at 0 to 890000 in the trace (locked at 521000)"])

    def test_work_at_fixed_clocks(self):
        """With no frequency events in the trace (the clocks don't change), each core's running
        time is scaled by its locked frequency and capacity."""
        capacities = BoostTest.PIXEL_8_PRO
        fixed = measure.fixed_frequencies({"policy0": 1425000, "policy8": 1745000},
                                          {"policy0": [0, 1, 2, 3], "policy8": [8]})
        self.assertEqual(fixed, {0: 1425000, 1: 1425000, 2: 1425000, 3: 1425000, 8: 1745000})
        running = {"main": {0: 10_000_000, 8: 2_000_000}, "render": {8: 4_000_000}}
        work = measure.fixed_work_ms(running, capacities, fixed)
        self.assertAlmostEqual(work["main"], 10 * 1425000 / 1704000 * 255 / 1024
                               + 2 * 1745000 / 2914000, places=6)
        self.assertAlmostEqual(work["render"], 4 * 1745000 / 2914000, places=6)
        # A core without a locked frequency can't be scaled: no work rather than a wrong one.
        self.assertIsNone(measure.fixed_work_ms({"main": {4: 1}}, capacities, fixed))

    def test_each_threads_time_by_cluster(self):
        capacities = BoostTest.PIXEL_8_PRO
        running = {"main": {0: 6_000_000, 5: 3_000_000, 8: 1_000_000}, "render": {4: 2_000_000}}
        self.assertEqual(measure.cluster_ms(running, capacities),
                         {"main": {"little": 6.0, "mid": 3.0, "big": 1.0},
                          "render": {"little": 0.0, "mid": 2.0, "big": 0.0}})

    def test_summarize_prints_the_clusters(self):
        traces = [{"cluster_ms": {"main": {"little": 10.0, "mid": 5.0, "big": 1.0},
                                  "render": {"little": 2.0, "mid": 8.0, "big": 0.0}}},
                  {"cluster_ms": {"main": {"little": 20.0, "mid": 7.0, "big": 3.0},
                                  "render": {"little": 4.0, "mid": 6.0, "big": 0.0}}},
                  {}]
        self.assertEqual(summarize.cluster_summary(traces),
                         "main little 15 mid 6 big 2 ms | render little 3 mid 7 big 0 ms")
        self.assertIsNone(summarize.cluster_summary([{}]))

    def test_fixed_placement(self):
        """The top-app group's uclamp.min (`placement=`, D641) is read back with the clocks and
        must hold too."""
        state = LOCK_STATE + "locked:placement 512\ntouched:placement\n"
        expected = measure.locked_clocks(state)
        self.assertEqual(expected["placement"], 512)
        clocks = measure.parse_clocks(CLOCKS_OUTPUT + "placement 512\n")
        self.assertEqual(clocks["placement"], {"value": 512})
        self.assertEqual(measure.clocks_moved(expected, clocks), [])
        moved = measure.parse_clocks(CLOCKS_OUTPUT + "placement 1\n")
        self.assertEqual(measure.clocks_moved(expected, moved),
                         ["placement: top-app uclamp.min 1 (locked at 512)"])
        self.assertEqual(measure.clocks_moved(expected, measure.parse_clocks(CLOCKS_OUTPUT)),
                         ["placement: not read"])
        # The flag and the state must agree: a run can't measure placement unawares.
        self.assertEqual(measure.check_placement(expected, True), 512)
        self.assertIsNone(measure.check_placement(measure.locked_clocks(LOCK_STATE), False))
        self.assertIsNone(measure.check_placement(None, False))
        for given, flag in ((measure.locked_clocks(LOCK_STATE), True), (expected, False),
                            (None, True)):
            with self.assertRaises(SystemExit):
                measure.check_placement(given, flag)

    def test_the_parser_takes_locked_clocks(self):
        with tempfile.TemporaryDirectory() as directory:
            state = os.path.join(directory, "clocks.state")
            with open(state, "w") as file:
                file.write(LOCK_STATE)
            self.assertEqual(measure.load_locked_clocks(state)["policy8"], 1745000)
            self.assertIsNone(measure.load_locked_clocks(None))


# `simpleperf stat -p <bench-rs> --per-thread -e instructions:u,instructions:k,cpu-cycles --csv
# --verbose` on the Pixel 8 Pro (Android 17), in the stress animation, three of its threads kept:
# each event's row per thread and CPU (count, time enabled, time running), then the summary.
SIMPLEPERF_STAT = """\
Performance counter statistics,
instructions,tid,32617,cpu,0,count,0,time_enabled,751816725,time running,0,id,7839,
instructions,tid,32617,cpu,1,count,0,time_enabled,751808913,time running,0,id,7840,
instructions,tid,32617,cpu,2,count,0,time_enabled,751802119,time running,0,id,7841,
instructions,tid,32617,cpu,3,count,0,time_enabled,751795974,time running,0,id,7842,
instructions,tid,32617,cpu,4,count,253015366,time_enabled,751800653,time running,131074416,id,7843,
instructions,tid,32617,cpu,5,count,328274118,time_enabled,751794062,time running,165378822,id,7844,
instructions,tid,32617,cpu,6,count,421714644,time_enabled,751786046,time running,205178097,id,7845,
instructions,tid,32617,cpu,7,count,455024954,time_enabled,751778030,time running,222120205,id,7846,
instructions,tid,32617,cpu,8,count,60586738,time_enabled,751767816,time running,28049113,id,7847,
instructions,tid,32632,cpu,0,count,0,time_enabled,1581024068,time running,0,id,7956,
instructions,tid,32632,cpu,1,count,0,time_enabled,1581014262,time running,0,id,7957,
instructions,tid,32632,cpu,2,count,0,time_enabled,1581004822,time running,0,id,7958,
instructions,tid,32632,cpu,3,count,0,time_enabled,1580995422,time running,0,id,7959,
instructions,tid,32632,cpu,4,count,440711806,time_enabled,1580986755,time running,243936159,id,7960,
instructions,tid,32632,cpu,5,count,197056551,time_enabled,1580977274,time running,114513912,id,7961,
instructions,tid,32632,cpu,6,count,175240524,time_enabled,1580964660,time running,100921622,id,7962,
instructions,tid,32632,cpu,7,count,108415457,time_enabled,1580680847,time running,57242186,id,7963,
instructions,tid,32632,cpu,8,count,2890977156,time_enabled,1580634419,time running,1064380729,id,7964,
instructions,tid,32640,cpu,0,count,0,time_enabled,1721124258,time running,0,id,8001,
instructions,tid,32640,cpu,1,count,0,time_enabled,1721116893,time running,0,id,8002,
instructions,tid,32640,cpu,2,count,0,time_enabled,1721112499,time running,0,id,8003,
instructions,tid,32640,cpu,3,count,0,time_enabled,1721107494,time running,0,id,8004,
instructions,tid,32640,cpu,4,count,935428791,time_enabled,1721103262,time running,185383789,id,8005,
instructions,tid,32640,cpu,5,count,962474641,time_enabled,1721099640,time running,194777264,id,8006,
instructions,tid,32640,cpu,6,count,2224017755,time_enabled,1721095082,time running,434768510,id,8007,
instructions,tid,32640,cpu,7,count,3974823726,time_enabled,1721097563,time running,803622062,id,8008,
instructions,tid,32640,cpu,8,count,578934590,time_enabled,1721092396,time running,102545938,id,8009,
instructions,tid,32617,cpu,0,count,0,time_enabled,750928094,time running,0,id,8082,
instructions,tid,32617,cpu,1,count,0,time_enabled,750928094,time running,0,id,8083,
instructions,tid,32617,cpu,2,count,0,time_enabled,750928094,time running,0,id,8084,
instructions,tid,32617,cpu,3,count,0,time_enabled,750928094,time running,0,id,8085,
instructions,tid,32617,cpu,4,count,7335457,time_enabled,750928094,time running,130201857,id,8086,
instructions,tid,32617,cpu,5,count,8357280,time_enabled,750928094,time running,165378822,id,8087,
instructions,tid,32617,cpu,6,count,11515240,time_enabled,750928094,time running,205178097,id,8088,
instructions,tid,32617,cpu,7,count,12005168,time_enabled,750928094,time running,222120205,id,8089,
instructions,tid,32617,cpu,8,count,1432917,time_enabled,750928094,time running,28049113,id,8090,
instructions,tid,32632,cpu,0,count,0,time_enabled,1578621276,time running,0,id,8199,
instructions,tid,32632,cpu,1,count,0,time_enabled,1578361185,time running,0,id,8200,
instructions,tid,32632,cpu,2,count,0,time_enabled,1578238016,time running,0,id,8201,
instructions,tid,32632,cpu,3,count,0,time_enabled,1578205220,time running,0,id,8202,
instructions,tid,32632,cpu,4,count,20620951,time_enabled,1578177632,time running,243936159,id,8203,
instructions,tid,32632,cpu,5,count,8408189,time_enabled,1578152729,time running,110123368,id,8204,
instructions,tid,32632,cpu,6,count,6105384,time_enabled,1578128274,time running,100921622,id,8205,
instructions,tid,32632,cpu,7,count,4138810,time_enabled,1578099465,time running,57242186,id,8206,
instructions,tid,32632,cpu,8,count,136717885,time_enabled,1578075173,time running,1065929394,id,8207,
instructions,tid,32640,cpu,0,count,0,time_enabled,1720304269,time running,0,id,8244,
instructions,tid,32640,cpu,1,count,0,time_enabled,1720319650,time running,0,id,8245,
instructions,tid,32640,cpu,2,count,0,time_enabled,1720333078,time running,0,id,8246,
instructions,tid,32640,cpu,3,count,0,time_enabled,1720333078,time running,0,id,8247,
instructions,tid,32640,cpu,4,count,16098870,time_enabled,1720333078,time running,185383789,id,8248,
instructions,tid,32640,cpu,5,count,16401729,time_enabled,1720333078,time running,194777264,id,8249,
instructions,tid,32640,cpu,6,count,31081842,time_enabled,1720333078,time running,434768510,id,8250,
instructions,tid,32640,cpu,7,count,62285288,time_enabled,1720333078,time running,802857577,id,8251,
instructions,tid,32640,cpu,8,count,4294975,time_enabled,1720333078,time running,102545938,id,8252,
cpu-cycles,tid,32617,cpu,0,count,0,time_enabled,750246739,time running,0,id,8325,
cpu-cycles,tid,32617,cpu,1,count,0,time_enabled,750238439,time running,0,id,8326,
cpu-cycles,tid,32617,cpu,2,count,0,time_enabled,750105464,time running,0,id,8327,
cpu-cycles,tid,32617,cpu,3,count,0,time_enabled,750134517,time running,0,id,8328,
cpu-cycles,tid,32617,cpu,4,count,219653716,time_enabled,750131343,time running,129405106,id,8329,
cpu-cycles,tid,32617,cpu,5,count,276463192,time_enabled,750113032,time running,165378822,id,8330,
cpu-cycles,tid,32617,cpu,6,count,348869889,time_enabled,750105015,time running,205178097,id,8331,
cpu-cycles,tid,32617,cpu,7,count,386024035,time_enabled,750098952,time running,222120205,id,8332,
cpu-cycles,tid,32617,cpu,8,count,54772067,time_enabled,750093460,time running,28049113,id,8333,
cpu-cycles,tid,32632,cpu,0,count,0,time_enabled,1573237038,time running,0,id,8442,
cpu-cycles,tid,32632,cpu,1,count,0,time_enabled,1573145730,time running,0,id,8443,
cpu-cycles,tid,32632,cpu,2,count,0,time_enabled,1573041319,time running,0,id,8444,
cpu-cycles,tid,32632,cpu,3,count,0,time_enabled,1573010842,time running,0,id,8445,
cpu-cycles,tid,32632,cpu,4,count,450346274,time_enabled,1572992491,time running,243936159,id,8446,
cpu-cycles,tid,32632,cpu,5,count,187143903,time_enabled,1572986591,time running,104277583,id,8447,
cpu-cycles,tid,32632,cpu,6,count,187807750,time_enabled,1572981748,time running,100921622,id,8448,
cpu-cycles,tid,32632,cpu,7,count,107743270,time_enabled,1572954526,time running,57242186,id,8449,
cpu-cycles,tid,32632,cpu,8,count,2659464213,time_enabled,1572942767,time running,1066565217,id,8450,
cpu-cycles,tid,32640,cpu,0,count,0,time_enabled,1715905628,time running,0,id,8487,
cpu-cycles,tid,32640,cpu,1,count,0,time_enabled,1715905628,time running,0,id,8488,
cpu-cycles,tid,32640,cpu,2,count,0,time_enabled,1715905628,time running,0,id,8489,
cpu-cycles,tid,32640,cpu,3,count,0,time_enabled,1715905628,time running,0,id,8490,
cpu-cycles,tid,32640,cpu,4,count,342894809,time_enabled,1715905628,time running,185383789,id,8491,
cpu-cycles,tid,32640,cpu,5,count,342373875,time_enabled,1715905628,time running,194777264,id,8492,
cpu-cycles,tid,32640,cpu,6,count,792469450,time_enabled,1715905628,time running,434768510,id,8493,
cpu-cycles,tid,32640,cpu,7,count,1397504760,time_enabled,1715905628,time running,798430127,id,8494,
cpu-cycles,tid,32640,cpu,8,count,172778552,time_enabled,1715905628,time running,102545938,id,8495,
hwuiTask0,32617,32640,8675679503,instructions:u,5.041,G/sec,
RenderThread,32617,32632,3812401494,instructions:u,2.411,G/sec,
ompose.bench.rs,32617,32617,1518615820,instructions:u,2.020,G/sec,
RenderThread,32617,32632,175991219,instructions:k,111.517,M/sec,
hwuiTask0,32617,32640,130162704,instructions:k,75.661,M/sec,
ompose.bench.rs,32617,32617,40646062,instructions:k,54.128,M/sec,
RenderThread,32617,32632,3592505410,cpu-cycles,2.283939,GHz,
hwuiTask0,32617,32640,3048021446,cpu-cycles,1.776334,GHz,
ompose.bench.rs,32617,32617,1285782899,cpu-cycles,1.714077,GHz,
Total test time,3.001741,seconds,
"""


# `simpleperf stat --print-hw-counter` on the Pixel 8 Pro: the Cortex-X3 (cpu 8) has one fewer.
HW_COUNTERS = """\
There are 4 CPU PMU hardware counters available on cpu 0.
There are 4 CPU PMU hardware counters available on cpu 7.
There are 3 CPU PMU hardware counters available on cpu 8.
"""


class CountersTest(unittest.TestCase):
    EVENTS = ("instructions:u", "instructions:k", "cpu-cycles")

    def test_per_thread_counts(self):
        parsed = measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS)
        self.assertEqual(parsed["seconds"], 3.001741)
        self.assertEqual(parsed["mismatches"], [])
        threads = {thread["tid"]: thread for thread in parsed["threads"]}
        self.assertEqual(sorted(threads), [32617, 32632, 32640])
        main = threads[32617]
        self.assertEqual((main["name"], main["pid"]), ("ompose.bench.rs", 32617))
        self.assertEqual(main["counts"], {"instructions:u": 1518615820, "instructions:k": 40646062,
                                          "cpu-cycles": 1285782899})
        # The summary's counts are the per-CPU rows' sums: no scaling.
        self.assertEqual(threads[32632]["counts"]["cpu-cycles"], 3592505410)

    def test_the_modifiers_rows_are_told_apart_by_their_order(self):
        """The per-CPU rows name the event without its modifier (`instructions` for both :u and
        :k): the n-th row of a thread and CPU is the n-th event of that name."""
        parsed = measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS)
        render = {thread["tid"]: thread for thread in parsed["threads"]}[32632]
        # Time running over time enabled, summed over the CPUs: 1 when never multiplexed.
        self.assertAlmostEqual(render["running"]["instructions:u"], 1580994608 / 1581024068)
        self.assertAlmostEqual(render["running"]["instructions:k"], 1578152729 / 1578621276)
        self.assertAlmostEqual(render["running"]["cpu-cycles"], 1572942767 / 1573237038)

    def test_multiplexed_counters_are_said(self):
        """Four events on a CPU with three counters (the X3): the counters take turns, each
        counting part of the time its thread ran."""
        text = (
            "Performance counter statistics,\n"
            "cpu-cycles,tid,10,cpu,0,count,600,time_enabled,1000,time running,600,id,1,\n"
            "cpu-cycles,tid,10,cpu,1,count,100,time_enabled,1000,time running,200,id,2,\n"
            "cpu-cycles,tid,11,cpu,0,count,0,time_enabled,0,time running,0,id,3,\n"
            "cpu-cycles,tid,11,cpu,1,count,0,time_enabled,0,time running,0,id,4,\n"
            "cpu-cycles,tid,10,cpu,0,count,50,time_enabled,1000,time running,800,id,5,\n"
            "cpu-cycles,tid,10,cpu,1,count,0,time_enabled,1000,time running,200,id,6,\n"
            "cpu-cycles,tid,11,cpu,0,count,0,time_enabled,0,time running,0,id,7,\n"
            "cpu-cycles,tid,11,cpu,1,count,0,time_enabled,0,time running,0,id,8,\n"
            "main,10,10,700,cpu-cycles:u,1.0,GHz,\n"
            "main,10,10,51,cpu-cycles:k,1.0,GHz,\n"
            "Total test time,1.000000,seconds,\n")
        parsed = measure.parse_simpleperf_stat(text, ("cpu-cycles:u", "cpu-cycles:k"))
        main, = parsed["threads"]
        self.assertEqual(main["running"], {"cpu-cycles:u": 0.8, "cpu-cycles:k": 1.0})
        # A summary that isn't the rows' sum is said (cpu-cycles:k's rows hold 50).
        self.assertEqual(parsed["mismatches"], ["tid 10 cpu-cycles:k: 51 in the summary, 50 by CPU"])
        # A thread that never ran (no summary row, no count) isn't listed.
        self.assertEqual([thread["tid"] for thread in parsed["threads"]], [10])

    def test_thread_groups(self):
        cases = [
            ("ompose.bench.rs", 100, 100, "main"), ("e.bench.compose", 7, 7, "main"),
            ("RenderThread", 101, 100, "RenderThread"), ("hwuiTask0", 102, 100, "hwuiTask"),
            ("hwuiTask1", 103, 100, "hwuiTask"), ("mali-event-hand", 104, 100, "GPU driver"),
            ("mali-mem-purge", 105, 100, "GPU driver"), ("GPU completion", 106, 100, "GPU driver"),
            ("binder:100_4", 107, 100, "binder"), ("Jit thread pool", 108, 100, "JIT"),
            ("HeapTaskDaemon", 109, 100, "GC"), ("ReferenceQueueD", 110, 100, "ART daemons"),
            ("FinalizerDaemon", 111, 100, "ART daemons"), ("FinalizerWatchd", 112, 100, "ART daemons"),
            ("Profile Saver", 113, 100, "ART daemons"), ("Signal Catcher", 114, 100, "ART daemons"),
            ("DefaultExecutor", 115, 100, "other"), ("TracingMuxer", 116, 100, "other"),
        ]
        for name, tid, pid, group in cases:
            self.assertEqual(measure.counter_group(name, tid, pid), group, name)

    def test_groups_and_the_process_total(self):
        parsed = measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS)
        groups = measure.group_counters(parsed["threads"], self.EVENTS)
        self.assertEqual(groups["main"]["threads"], {"ompose.bench.rs": 1})
        self.assertEqual(groups["hwuiTask"]["counts"]["instructions:u"], 8675679503)
        self.assertEqual(groups["total"]["counts"]["instructions:u"],
                         1518615820 + 3812401494 + 8675679503)
        self.assertEqual(groups["total"]["threads"],
                         {"ompose.bench.rs": 1, "RenderThread": 1, "hwuiTask0": 1})
        # The main thread and the RenderThread are always there, if only to show a zero.
        empty = measure.group_counters([], self.EVENTS)
        self.assertEqual(empty["main"]["counts"], {event: 0 for event in self.EVENTS})
        self.assertEqual(empty["RenderThread"]["threads"], {})
        self.assertEqual(list(groups), ["main", "RenderThread", "hwuiTask", "total"])

    def test_metrics_per_frame(self):
        counts = {"instructions:u": 3000, "instructions:k": 1000, "cpu-cycles": 2000}
        metrics = measure.counter_metrics(counts, frames=100)
        self.assertEqual(metrics["instructions"], 4000)
        self.assertEqual((metrics["instructions_u"], metrics["instructions_k"]), (3000, 1000))
        self.assertEqual(metrics["cycles"], 2000)
        self.assertEqual(metrics["ipc"], 2.0)
        self.assertEqual(metrics["kernel_share"], 0.25)
        self.assertEqual(metrics["instructions_per_frame"], 40.0)
        self.assertEqual(metrics["cycles_per_frame"], 20.0)
        self.assertEqual(metrics["per_frame"], {"instructions:u": 30.0, "instructions:k": 10.0,
                                                "cpu-cycles": 20.0})
        # Cycles split by mode are summed; an event of no modifier is the total.
        split = measure.counter_metrics({"instructions": 10, "cpu-cycles:u": 4, "cpu-cycles:k": 1},
                                        frames=0)
        self.assertEqual((split["instructions"], split["cycles"], split["ipc"]), (10, 5, 2.0))
        self.assertIsNone(split["instructions_u"])
        self.assertIsNone(split["kernel_share"])
        # No frame: nothing per frame.
        self.assertIsNone(split["instructions_per_frame"])
        self.assertEqual(split["per_frame"], {})
        self.assertIsNone(measure.counter_metrics({"instructions": 0, "cpu-cycles": 0}, 5)["ipc"])

    def test_a_run_and_its_checks(self):
        parsed = measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS)
        stats = measure.counter_run_stats(parsed, frames=360, events=self.EVENTS)
        self.assertEqual(stats["frames"], 360)
        self.assertEqual(stats["seconds"], 3.001741)
        main = stats["groups"]["main"]
        self.assertEqual(main["instructions"], 1518615820 + 40646062)
        self.assertAlmostEqual(main["instructions_per_frame"], (1518615820 + 40646062) / 360)
        self.assertAlmostEqual(main["ipc"], (1518615820 + 40646062) / 1285782899)
        self.assertEqual(stats["problems"], [])
        self.assertGreater(stats["running_ratio"]["min"], 0.999)
        # Zero frames, a thread group with no instruction, counters multiplexed: each said.
        multiplexed = json.loads(json.dumps(parsed))
        for thread in multiplexed["threads"]:
            if thread["name"] == "RenderThread":
                thread["running"]["cpu-cycles"] = 0.8
            if thread["tid"] == thread["pid"]:
                thread["counts"] = {event: 0 for event in self.EVENTS}
        stats = measure.counter_run_stats(multiplexed, frames=0, events=self.EVENTS)
        self.assertEqual(stats["running_ratio"], {"min": 0.8, "where": "RenderThread cpu-cycles"})
        self.assertEqual(stats["problems"], [
            "no frame drawn", "main: no instruction counted",
            "counters multiplexed: RenderThread cpu-cycles counted 80.0% of its running time"])

    def test_runs_summarized(self):
        def run(main, frames):
            return {"frames": frames, "seconds": 10.0, "problems": [],
                    "running_ratio": {"min": 1.0, "where": None},
                    "groups": {"main": {"instructions_per_frame": main, "ipc": 1.5,
                                        "threads": {"main": 1}},
                               "total": {"instructions_per_frame": 2 * main, "ipc": None,
                                         "threads": {"main": 1, "RenderThread": 1}}}}
        summary = measure.counters_summary([run(100.0, 10), run(110.0, 12), run(90.0, 11)])
        self.assertEqual(summary["runs"], 3)
        self.assertEqual(summary["frames"]["median"], 11)
        main = summary["groups"]["main"]["instructions_per_frame"]
        self.assertEqual((main["median"], main["min"], main["max"]), (100.0, 90.0, 110.0))
        self.assertAlmostEqual(main["spread"], 0.2)
        self.assertAlmostEqual(main["cv"], 10.0 / 100.0)
        self.assertEqual(summary["groups"]["total"]["ipc"]["median"], None)
        self.assertEqual(summary["groups"]["total"]["threads"], {"main": 1, "RenderThread": 1})
        self.assertEqual(summary["running_ratio_min"], 1.0)
        self.assertEqual(summary["problems"], [])

    def test_the_spread_of_runs(self):
        self.assertEqual(measure.run_spread([]), {"median": None, "min": None, "max": None,
                                                  "spread": None, "cv": None, "runs": 0})
        one = measure.run_spread([5.0])
        self.assertEqual((one["median"], one["spread"], one["cv"]), (5.0, 0.0, None))
        self.assertEqual(measure.run_spread([None, 2.0, 4.0])["median"], 3.0)

    def test_the_counting_command(self):
        """simpleperf counts the app's threads while its workload runs: the workload resets
        gfxinfo, says it's ready, waits (a line on its input, or a duration), then reads gfxinfo,
        so the frames counted are those drawn inside the counting window."""
        command = measure.counter_command(4242, "rs.compose.bench.rs", self.EVENTS,
                                          "/data/local/tmp/c.csv", "/data/local/tmp/g.txt")
        self.assertTrue(command.startswith(
            "simpleperf stat -p 4242 --per-thread -e instructions:u,instructions:k,cpu-cycles "
            "--csv --verbose -o /data/local/tmp/c.csv sh -c '"), command)
        self.assertIn("echo START $(date +%s%N); dumpsys gfxinfo rs.compose.bench.rs reset > /dev/null; "
                      "echo TASKS_READY $(ls /proc/4242/task); echo READY", command)
        self.assertIn("read line;", command)
        self.assertIn("dumpsys gfxinfo rs.compose.bench.rs > /data/local/tmp/g.txt; "
                      "for t in /proc/4242/task/*; do echo TASK_END ${t##*/} $(cat $t/comm); done; "
                      "echo END", command)
        timed = measure.counter_command(1, "p", self.EVENTS, "o", "g", duration_s=10)
        self.assertIn("sleep 10;", timed)
        self.assertNotIn("read line", timed)
        self.assertNotIn("framestats", timed)
        # An animation's: framestats at the reset and at the read, for its frame rate.
        animated = measure.counter_command(1, "p", self.EVENTS, "o", "g", duration_s=30,
                                           gfx_start="g0")
        self.assertIn("dumpsys gfxinfo p framestats reset > g0;", animated)
        self.assertIn("dumpsys gfxinfo p framestats > g;", animated)

    def test_the_workloads_window(self):
        """gfxinfo's reset and read each happen within their dumpsys: the frames' window runs
        from the middle of the first to the middle of the second."""
        out = "START 900000000\nREADY 1000000000\nEND 11000000000 11050000000\n"
        self.assertEqual(measure.workload_window(out),
                         {"start_ns": 900000000, "ready_ns": 1000000000, "stop_ns": 11000000000,
                          "end_ns": 11050000000, "frame_window_s": 10.075, "tasks_ready": [],
                          "tasks_end": {}})
        self.assertIsNone(measure.workload_window("START 1\nREADY 1\n"))

    def test_frames_in_the_counting_window(self):
        """An animation draws all the time: its frames in the counting window (simpleperf's, a
        little longer than gfxinfo's) are its frame rate times the window. A scroll's or a
        navigation's window starts and ends idle: its frames are gfxinfo's."""
        rate = {"fps": 120.0, "span_s": 29.9, "fps_last": 120.0}
        self.assertEqual(measure.frames_in_window(3588, 30.2, rate, continuous=True), 3624.0)
        self.assertEqual(measure.frames_in_window(3588, 30.2, rate, continuous=False), 3588)
        self.assertEqual(measure.frames_in_window(3588, None, rate, continuous=True), 3588)
        self.assertEqual(measure.frames_in_window(3588, 30.2, None, continuous=True), 3588)

    def test_the_frame_rate_from_framestats(self):
        """The frames counted since gfxinfo's reset completed after the latest frame listed at
        the reset and by the latest listed at the read (framestats' `FrameCompleted`, the same
        clock): the rate over that span, and over the read's last frames listed."""
        def stats(completed):
            rows = "".join(f"0,{t - 8_000_000},{t},\n" for t in completed)
            return f"---PROFILEDATA---\nFlags,IntendedVsync,FrameCompleted,\n{rows}---PROFILEDATA---\n"
        start = stats([1_000_000_000, 1_008_333_333])
        end = stats([1_008_333_333 + 8_333_333 * k for k in range(2401, 2521)])
        rate = measure.framestats_rate(start, end, frames=2520)
        self.assertAlmostEqual(rate["span_s"], 2520 * 8_333_333 / 1e9)
        self.assertAlmostEqual(rate["fps"], 2520 / rate["span_s"])
        self.assertAlmostEqual(rate["fps_last"], 119 / (119 * 8_333_333 / 1e9))
        self.assertIsNone(measure.framestats_rate("", end, frames=2520))
        self.assertIsNone(measure.framestats_rate(start, end, frames=0))

    def test_hw_counters(self):
        self.assertEqual(measure.hw_counters(HW_COUNTERS), {0: 4, 7: 4, 8: 3})
        self.assertEqual(measure.hw_counters(""), {})

    def test_scenarios_that_can_be_counted(self):
        """A cold start can't be: simpleperf attaches to a process that exists, after its
        threads have started."""
        kept, skipped = measure.counter_scenarios(
            ["startup", "scroll", "animation", "stress_startup", "stress_animation", "navigation"])
        self.assertEqual(kept, ["scroll", "animation", "stress_animation", "navigation"])
        self.assertEqual(sorted(skipped), ["startup", "stress_startup"])

    def test_the_parser_takes_counters(self):
        """--counters needs no trace processor; the runs and events are configurable."""
        captured = {}

        def configure(arguments, source):
            captured.update(vars(arguments))
            raise SystemExit(0)

        original = measure.configure
        measure.configure = configure
        try:
            with self.assertRaises(SystemExit):
                measure.main(["--out", "out", "--counters", "--counter-runs", "3"])
            self.assertTrue(captured["counters"])
            self.assertEqual(captured["counter_runs"], 3)
            self.assertEqual(captured["counter_events"], "instructions:u,instructions:k,cpu-cycles")
            captured.clear()
            with self.assertRaises(SystemExit):
                measure.main(["--out", "out", "--counters"])
            self.assertEqual(captured["counter_runs"], 15)
            # Without --counters the traces need one.
            captured.clear()
            with self.assertRaises(SystemExit):
                with contextlib.redirect_stderr(io.StringIO()):
                    measure.main(["--out", "out"])
            self.assertEqual(captured, {})
        finally:
            measure.configure = original

    def test_summarize_prints_the_counters(self):
        def spread(median):
            return {"median": median, "min": median, "max": median, "spread": 0.01, "cv": 0.005,
                    "runs": 3}
        groups = {group: {"instructions_per_frame": spread(value), "ipc": spread(1.25),
                          "kernel_share": spread(0.05), "cycles_per_frame": spread(value / 2),
                          "threads": {"main": 1}}
                  for group, value in (("main", 4.0e6), ("RenderThread", 6.5e6), ("total", 2.0e7))}
        counters = {"events": list(self.EVENTS), "runs": 3, "skipped": {},
                    "scenarios": {"stress_animation": {"subjects": {
                        "compose": {"summary": {"runs": 3, "frames": spread(1200),
                                                "groups": groups, "running_ratio_min": 1.0,
                                                "problems": []}}}}}}
        lines = summarize.counters_lines(counters)
        self.assertIn("counters stress_animation compose (3 runs, 1200 frames): "
                      "main 4.00 M/frame (spread 1.0%) IPC 1.25 kernel 5%", lines[0])
        self.assertIn("RenderThread 6.50 M/frame (spread 1.0%) IPC 1.25", lines[0])
        self.assertIn("total 20.00 M/frame", lines[0])
        # A navigation's: per navigation too.
        groups["total"]["instructions_per_navigation"] = spread(2.1e8)
        lines = summarize.counters_lines(counters)
        self.assertIn("total 20.00 M/frame (spread 1.0%) IPC 1.25 kernel 5%, "
                      "210.0 M/navigation (spread 1.0%)", lines[0])
        # The calibration (the feed at rest) in M instructions per second; an animation's frame
        # scaling with its uncertainty.
        idle = {group: {"instructions_per_second": spread(value), "ipc": spread(0.5),
                        "kernel_share": spread(0.4), "threads": {}}
                for group, value in (("main", 1.0e6), ("total", 3.0e6))}
        counters["scenarios"] = {"idle": {"subjects": {"rs": {"summary": {
            "runs": 3, "frames": spread(0), "groups": idle, "running_ratio_min": 1.0,
            "problems": []}}}}}
        counters["scenarios"]["stress_animation"] = {"subjects": {"compose": {"summary": {
            "runs": 3, "frames": spread(1200), "groups": groups, "running_ratio_min": 1.0,
            "frame_scale_uncertainty_max": 0.004, "problems": ["run 0: x"]}}}}
        lines = summarize.counters_lines(counters)
        self.assertEqual(lines[0], "counters idle rs (3 runs, at rest): main 1.00 M/s "
                                   "(spread 1.0%) IPC 0.50 kernel 40% | total 3.00 M/s "
                                   "(spread 1.0%) IPC 0.50 kernel 40%")
        self.assertIn("(3 runs, 1200 frames, frames scaled ±0.40%)", lines[1])
        self.assertEqual(lines[2], "counters stress_animation compose problems: run 0: x")


class FakeCounting:
    """`count_while`'s device: a fake `adb shell` process (the workload's output before and after
    the host's stop line) and a fake `shell` (its commands recorded)."""

    def __init__(self, before, after="", stderr="", returncode=0, csv=None, frames=100,
                 frames_after=None):
        self.before, self.after, self.stderr, self.code = before, after, stderr, returncode
        self.csv = csv if csv is not None else SIMPLEPERF_STAT
        self.frames = frames
        self.frames_after = frames if frames_after is None else frames_after
        self.commands = []
        self.killed = False
        self.stdin = io.StringIO()
        self.stdout = None
        self.returncode = None

    def shell(self, command):
        self.commands.append(command)
        if command.startswith("pidof"):
            return "32617\n"
        if command.startswith("cat ") and command.endswith(".csv"):
            return self.csv
        if command.startswith("cat "):
            return f"Total frames rendered: {self.frames}\n"
        if command.startswith("dumpsys gfxinfo") and self.stdout is not None:
            return f"Total frames rendered: {self.frames_after}\n"
        return ""

    def popen(self, argv, **kwargs):
        self.commands.append("popen " + argv[-1][:20])
        self.stdout = io.StringIO(self.before)
        self.stdin.close = lambda: None
        return self

    def communicate(self, timeout=None):
        self.returncode = self.code
        return self.after, self.stderr

    def poll(self):
        return self.returncode

    def kill(self):
        self.killed = True
        self.returncode = -9


class CountersReviewTest(unittest.TestCase):
    EVENTS = ("instructions:u", "instructions:k", "cpu-cycles")
    BEFORE = "START 100\nTASKS_READY 32617 32632 32640 32999\nREADY 200\n"
    AFTER = ("TASK_END 32617 ompose.bench.rs\nTASK_END 32632 RenderThread\n"
             "TASK_END 32640 hwuiTask0\nTASK_END 33001 binder:32617_4\nEND 10300 10400\n")

    def test_the_handshake(self):
        """The host's input comes after READY and before the stop line; the counts, the frames
        and the threads at both ends come back; the device's files are removed."""
        device = FakeCounting(self.BEFORE, self.AFTER)
        seen = {}

        def body():
            seen["stdin_at_body"] = device.stdin.getvalue()

        parsed, frames, gfx, window = measure.count_while(
            "rs.compose.bench.rs", self.EVENTS, body, run_shell=device.shell, popen=device.popen)
        self.assertEqual(seen["stdin_at_body"], "")
        self.assertEqual(device.stdin.getvalue(), "stop\n")
        self.assertEqual(frames, 100)
        self.assertEqual(gfx["frames"], 100)
        self.assertEqual(len(parsed["threads"]), 3)
        self.assertEqual(window["tasks_ready"], [32617, 32632, 32640, 32999])
        self.assertEqual(window["tasks_end"][33001], "binder:32617_4")
        self.assertEqual(window["frames_after"], 100)
        self.assertTrue(any(command.startswith("rm -f") for command in
                            device.commands[device.commands.index("cat " + measure.COUNTER_FILES["gfx"]):]))
        self.assertFalse(device.killed)
        # With a duration, nothing is written to the workload.
        device = FakeCounting(self.BEFORE, self.AFTER)
        measure.count_while("p", self.EVENTS, duration_s=30, run_shell=device.shell,
                            popen=device.popen)
        self.assertEqual(device.stdin.getvalue(), "")

    def test_the_handshake_fails(self):
        """No READY (simpleperf failed): the process is killed, the run stops, the files go."""
        device = FakeCounting("simpleperf E cmd_stat.cpp] failed to open\n", stderr="boom")
        with self.assertRaises(SystemExit):
            measure.count_while("p", self.EVENTS, lambda: self.fail("no body without READY"),
                                run_shell=device.shell, popen=device.popen)
        self.assertTrue(device.killed)
        self.assertTrue(device.commands[-1].startswith("rm -f"))
        # A workload that never ends (no END): the run stops too.
        device = FakeCounting(self.BEFORE, "TASK_END 1 x\n")
        with self.assertRaises(SystemExit):
            measure.count_while("p", self.EVENTS, run_shell=device.shell, popen=device.popen)
        self.assertTrue(device.commands[-1].startswith("rm -f"))

    def test_threads_born_or_ended_in_the_window(self):
        window = measure.workload_window(self.BEFORE + self.AFTER)
        parsed = measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS)
        self.assertEqual(measure.thread_problems(window, parsed), [
            "thread binder:32617_4 (33001) started in the window: counted with its creator's",
            "thread 32999 ended in the window"])
        # A thread alive at both ends with no row of simpleperf's isn't counted at all.
        window["tasks_ready"].append(33002)
        window["tasks_end"][33002] = "Jit thread pool"
        self.assertIn("thread Jit thread pool (33002) has no simpleperf row",
                      measure.thread_problems(window, parsed))
        # Threads with rows but no count (idle) aren't in the summary, and are fine.
        parsed["row_tids"].append(33002)
        self.assertNotIn("thread Jit thread pool (33002) has no simpleperf row",
                         measure.thread_problems(window, parsed))

    def test_rows_and_mismatches(self):
        parsed = measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS)
        self.assertEqual(sorted(parsed["row_tids"]), [32617, 32632, 32640])
        changed = SIMPLEPERF_STAT.replace("ompose.bench.rs,32617,32617,40646062,instructions:k",
                                          "ompose.bench.rs,32617,32617,40646063,instructions:k")
        self.assertEqual(measure.parse_simpleperf_stat(changed, self.EVENTS)["mismatches"],
                         ["tid 32617 instructions:k: 40646063 in the summary, 40646062 by CPU"])

    def test_rows_not_counted_and_names_with_commas(self):
        text = ("Performance counter statistics,\n"
                "a,b thread,10,11,<not counted>,instructions:u,0,/sec,\n"
                "a,b thread,10,11,5,cpu-cycles,1.0,GHz,\n"
                "x,10,12,<not supported>,instructions:u,0,/sec,\n"
                "Total test time,1.000000,seconds,\n")
        parsed = measure.parse_simpleperf_stat(text, ("instructions:u", "cpu-cycles"))
        threads = {thread["tid"]: thread for thread in parsed["threads"]}
        self.assertEqual(threads[11]["name"], "a,b thread")
        self.assertEqual(threads[11]["counts"], {"instructions:u": None, "cpu-cycles": 5})
        self.assertEqual(threads[12]["counts"]["instructions:u"], None)
        self.assertEqual(parsed["not_counted"], ["a,b thread (11) instructions:u: <not counted>",
                                                 "x (12) instructions:u: <not supported>"])
        stats = measure.counter_run_stats(parsed, frames=10, events=("instructions:u", "cpu-cycles"))
        self.assertIn("a,b thread (11) instructions:u: <not counted>", stats["problems"])
        # A count not counted adds nothing.
        self.assertEqual(stats["groups"]["total"]["counts"]["instructions:u"], 0)

    def test_binder_threads_by_kind(self):
        self.assertEqual(measure.normalize_thread_name("binder:8177_3"), "binder")
        self.assertEqual(measure.normalize_thread_name("HwBinder:8177_1"), "HwBinder")
        self.assertEqual(measure.normalize_thread_name("RenderThread"), "RenderThread")
        threads = [{"tid": 2, "pid": 1, "name": "binder:1_2", "counts": {"cpu-cycles": 1}},
                   {"tid": 3, "pid": 1, "name": "binder:1_3", "counts": {"cpu-cycles": 2}}]
        groups = measure.group_counters(threads, ("cpu-cycles",))
        self.assertEqual(groups["binder"]["threads"], {"binder": 2})

    def test_simpleperfs_warnings(self):
        warning = ("simpleperf W cmd_stat.cpp:1240] It seems the number of hardware events are "
                   "more than the number of\navailable CPU PMU hardware counters. That will "
                   "trigger hardware counter\nmultiplexing.")
        self.assertEqual(measure.stderr_problems(warning),
                         ["simpleperf warned of multiplexing (more events than counters)"])
        self.assertEqual(measure.stderr_problems(""), [])
        self.assertEqual(measure.stderr_problems("simpleperf E x.cpp] failed"),
                         ["simpleperf said: simpleperf E x.cpp] failed"])

    def test_continuous_scenarios(self):
        self.assertTrue(measure.counter_continuous("animation"))
        self.assertTrue(measure.counter_continuous("stress_animation"))
        for name in ("scroll", "stress_scroll", "navigation", "idle"):
            self.assertFalse(measure.counter_continuous(name), name)

    def test_the_frame_scalings_uncertainty(self):
        """One frame of the span's, plus the window's part outside the span (extrapolated at
        the rate) times how far the rate drifted (its last frames' against the whole span's)."""
        rate = {"fps": 120.0, "span_s": 30.0, "fps_last": 118.8}
        self.assertAlmostEqual(measure.frame_scale_uncertainty(rate, 30.3),
                               1 / 3600 + (0.3 / 30.3) * (1.2 / 120.0))
        self.assertIsNone(measure.frame_scale_uncertainty(None, 30.3))
        self.assertIsNone(measure.frame_scale_uncertainty(rate, None))

    def test_idle_edges(self):
        """Before counting, the host waits until gfxinfo's frames stop growing; after it, any
        frame drawn after the window's last read means the window ended drawing."""
        counts = iter([10, 12, 14, 15, 15, 15, 15, 15])
        slept = []
        result = measure.wait_idle(lambda: next(counts), slept.append, quiet_s=1.0, poll_s=0.25,
                                   limit_s=10)
        self.assertEqual(result, {"waited_s": 1.75, "idle": True})
        never = measure.wait_idle(iter(range(100)).__next__, lambda s: None, quiet_s=1.0,
                                  poll_s=0.5, limit_s=2)
        self.assertEqual(never, {"waited_s": 2.0, "idle": False})
        self.assertEqual(measure.edge_problems({"idle": True}, 100, 100), [])
        self.assertEqual(measure.edge_problems({"idle": False, "waited_s": 2.0}, 100, 103), [
            "the app still drew after waiting 2.0 s for it to idle before the window",
            "3 frames drawn after the window's last read: it ended drawing"])

    def test_a_missing_group_counts_as_zero(self):
        def run(hwui):
            groups = {"main": {"instructions_per_frame": 10.0, "ipc": 1.0, "threads": {}},
                      "total": {"instructions_per_frame": 10.0 + (hwui or 0), "ipc": 1.0,
                                "threads": {}}}
            if hwui is not None:
                groups["hwuiTask"] = {"instructions_per_frame": hwui, "ipc": 2.0, "threads": {}}
            return {"frames": 10, "seconds": 1.0, "problems": [],
                    "running_ratio": {"min": 1.0, "where": None}, "groups": groups}
        summary = measure.counters_summary([run(None), run(4.0), run(None)])
        hwui = summary["groups"]["hwuiTask"]
        self.assertEqual(hwui["instructions_per_frame"]["median"], 0)
        self.assertEqual(hwui["instructions_per_frame"]["runs"], 3)
        self.assertEqual(hwui["ipc"]["runs"], 1)

    def test_per_second(self):
        metrics = measure.counter_run_stats(
            measure.parse_simpleperf_stat(SIMPLEPERF_STAT, self.EVENTS), 0, self.EVENTS)
        self.assertAlmostEqual(metrics["groups"]["main"]["instructions_per_second"],
                               (1518615820 + 40646062) / 3.001741)


if __name__ == "__main__":
    unittest.main()
