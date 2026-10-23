#!/usr/bin/env python3
"""measure.py's parsing, on captured output (no device): python3 android/bench/test_measure.py"""

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


if __name__ == "__main__":
    unittest.main()
