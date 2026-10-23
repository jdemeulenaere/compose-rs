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


if __name__ == "__main__":
    unittest.main()
