#!/usr/bin/env python3
"""measure.py's parsing, on captured output (no device): python3 android/bench/test_measure.py"""

import os
import tempfile
import unittest
import zipfile

import measure

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
    def test_base_apk_path(self):
        output = ("package:/data/app/~~a==/rs.compose.bench.rs-b==/base.apk\n"
                  "package:/data/app/~~a==/rs.compose.bench.rs-b==/split_config.arm64_v8a.apk\n")
        self.assertEqual(measure.base_apk_path(output),
                         "/data/app/~~a==/rs.compose.bench.rs-b==/base.apk")
        self.assertIsNone(measure.base_apk_path(""))

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
            stale = measure.compare_apks(measure.apk_summary(stale_path), built)
            self.assertTrue(stale[0].startswith("libraries"), stale)
            dex = measure.compare_apks(measure.apk_summary(dex_path), built)
            self.assertEqual(len(dex), 1)
            self.assertTrue(dex[0].startswith("APK"), dex)
            self.assertEqual(measure.compare_apks(built, None), [])


if __name__ == "__main__":
    unittest.main()
