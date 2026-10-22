#!/usr/bin/env python3
"""Measures the benchmark apps (bench-compose: Kotlin Compose; bench-rs: compose-rs) on a
connected device, interleaving the two apps run by run, and writes a JSON report.

    measure.py --trace-processor PATH --out DIR [--runs N] [--compile MODE] [--allow-stale]

Before measuring, the installed APKs are checked against the ones built in this checkout
(android/bench-*/build/outputs/apk/release/): the device ABI's native libraries' SHA-1s must
match (an install from Android Studio, or as splits, holds only those), or the run stops
(--allow-stale measures anyway). The report records each APK's size and SHA-1 and its
libraries', and each app's ART compilation state (`dumpsys package dexopt`) at the start and the
end of the run.

--compile (`cmd package compile -m MODE -f`, before measuring):
  speed          (default) everything compiled ahead of time; docs/12's rounds 1 to 3 used it
  speed-profile  each app is reset, run through its screens so ART records a profile, then
                 compiled from that profile: what a Baseline or Cloud Profile gives a Play
                 install, with an app image. Compose's feed starts faster this way than with
                 speed (docs/14). compose-rs's Kotlin glue leaves no profile worth compiling, so
                 its state stays verify (the dexopt log shows it): its startup doesn't depend on
                 it, but its replay runs JIT code
  verify         no ahead-of-time code: a fresh install with no profile

Scenarios (docs/12-android-performance.md):
  startup      cold starts of the feed and of the big screen (`am start -W`: TotalTime), and a
               Perfetto trace of a cold start of each for the first frame's breakdown
  memory       PSS and its parts (`dumpsys meminfo`) after start, and after scrolling
  scroll       10 flings through the feed, after a warm-up swipe left out: frames
               (`dumpsys gfxinfo`) and jank (FrameTimeline: the app's or the compositor's, and
               frames presented at another vsync than expected), doFrames starting late,
               compose-rs's prefetches, and whether Choreographer recovered from buffer stuffing
  animation    10 s of the animated header, no input: the same

Before each run, our other apps (`rs.compose.*`) are force-stopped (D343).
"""

import argparse
import hashlib
import json
import os
import re
import statistics
import subprocess
import time
import zipfile

APPS = {"compose": "rs.compose.bench.compose", "rs": "rs.compose.bench.rs"}
# Our apps' packages: those not under test are stopped before each run.
OUR_PREFIX = "rs.compose."
# Where Gradle puts each app's release APK, relative to android/.
BUILT_APKS = {
    "compose": "bench-compose/build/outputs/apk/release/bench-compose-release.apk",
    "rs": "bench-rs/build/outputs/apk/release/bench-rs-release.apk",
}
ANDROID_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def adb(*args, check=True):
    result = subprocess.run(["adb", *args], capture_output=True, text=True)
    if check and result.returncode != 0:
        raise RuntimeError(f"adb {' '.join(args)}: {result.stderr}")
    return result.stdout


def shell(command):
    return adb("shell", command)


def start(package, screen, wait=True):
    flags = "-W " if wait else ""
    return shell(f"am start {flags}-n {package}/.MainActivity -e screen {screen}")


def stop(package):
    shell(f"am force-stop {package}")


def packages_to_stop(pm_list_output, package):
    """Our other apps (`rs.compose.*`, from `pm list packages`' `package:<name>` lines): all but
    `package`, the one under test."""
    names = [line.strip()[len("package:"):] for line in pm_list_output.splitlines()
             if line.strip().startswith("package:")]
    return sorted(name for name in names if name.startswith(OUR_PREFIX) and name != package)


def stop_all_but(package):
    """Force-stops our other apps, so none runs beside the one measured: the other benchmark
    app left cached by the run before, the demo's animation in the background (D343)."""
    for other in packages_to_stop(shell(f"pm list packages {OUR_PREFIX}"), package):
        stop(other)


def fresh(package):
    """`package` stopped, and our other apps too: each run starts from the same state."""
    stop_all_but(package)
    stop(package)


def cold_start(package, screen):
    fresh(package)
    time.sleep(1.0)
    output = start(package, screen)
    match = re.search(r"TotalTime: (\d+)", output)
    return int(match.group(1)) if match else None


def apk_summary(path):
    """An APK's size and SHA-1, and each native library's."""
    digest = hashlib.sha1()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    libraries = {}
    with zipfile.ZipFile(path) as archive:
        for entry in archive.infolist():
            if entry.filename.startswith("lib/") and entry.filename.endswith(".so"):
                libraries[entry.filename] = {
                    "bytes": entry.file_size,
                    "sha1": hashlib.sha1(archive.read(entry)).hexdigest(),
                }
    return {"bytes": os.path.getsize(path), "sha1": digest.hexdigest(), "libraries": libraries}


def apk_paths(pm_path_output):
    """The APKs an app is installed as (the base, then any splits), from `pm path`'s
    `package:<path>` lines."""
    paths = [line.strip()[len("package:"):] for line in pm_path_output.splitlines()
             if line.strip().startswith("package:")]
    return sorted(paths, key=lambda path: not path.endswith("/base.apk"))


def installed_summary(apk_files):
    """An installed app's APKs, the base first: their total size, the base's SHA-1, the number
    of splits, and the native libraries of all of them."""
    apks = [apk_summary(path) for path in apk_files]
    libraries = {}
    for apk in apks:
        libraries.update(apk["libraries"])
    return {"bytes": sum(apk["bytes"] for apk in apks), "sha1": apks[0]["sha1"],
            "splits": len(apks) - 1, "libraries": libraries}


def abi_libraries(apk, abi):
    """An APK's libraries for one ABI (`lib/<abi>/`)."""
    return {name: value for name, value in apk["libraries"].items()
            if name.startswith(f"lib/{abi}/")}


def compare_apks(installed, built, abi):
    """What differs between the installed app and the built APK, as (whether the device ABI's
    libraries differ, the problems). Only `lib/<abi>/` counts: an install from Android Studio
    or as splits holds the device's ABI only. Nothing if no APK was built."""
    if built is None:
        return False, []
    problems = []
    installed_libraries = abi_libraries(installed, abi)
    built_libraries = abi_libraries(built, abi)
    # A built APK with libraries, but none for the device, is as wrong as different ones.
    libraries_differ = installed_libraries != built_libraries or (
        not built_libraries and bool(built["libraries"]))
    if libraries_differ:
        def short(libraries):
            return {name: value["sha1"][:12] for name, value in libraries.items()} or "none"
        problems.append(f"{abi} libraries {short(installed_libraries)} installed, "
                        f"{short(built_libraries)} built")
    if installed["sha1"] != built["sha1"]:
        splits = f" (and {installed['splits']} splits)" if installed["splits"] else ""
        problems.append(f"APK {installed['sha1'][:12]}{splits} installed, "
                        f"{built['sha1'][:12]} built")
    return libraries_differ, problems


def check_apks(out_dir, abi, allow_stale):
    """Pulls each installed app's APKs and records their size and checksums. Stops if the
    device ABI's libraries aren't those of the APK built in this checkout, unless allow_stale;
    warns if only the rest differs."""
    apks = {}
    for name, package in APPS.items():
        # `pm path` fails for a package that isn't installed.
        device_paths = apk_paths(adb("shell", f"pm path {package}", check=False))
        if not device_paths:
            raise SystemExit(f"{package} isn't installed")
        local_paths = []
        for index, device_path in enumerate(device_paths):
            local_paths.append(os.path.join(out_dir, f"installed-{package}-{index}.apk"))
            adb("pull", device_path, local_paths[-1])
        installed = installed_summary(local_paths)
        for path in local_paths:
            os.remove(path)
        built_path = os.path.join(ANDROID_DIR, BUILT_APKS[name])
        built = apk_summary(built_path) if os.path.exists(built_path) else None
        libraries_differ, problems = compare_apks(installed, built, abi)
        apks[name] = {"installed": installed, "built": built, "built_path": built_path,
                      "problems": problems}
        print(f"apk {name}: {installed['bytes']} bytes, sha1 {installed['sha1'][:12]}"
              + (f", {installed['splits']} splits" if installed["splits"] else "") + "; "
              + ", ".join(f"{library} {value['bytes']} bytes, sha1 {value['sha1'][:12]}"
                          for library, value in abi_libraries(installed, abi).items()))
        if built is None:
            print(f"apk {name}: no APK built at {built_path}: recorded the installed one only")
        elif libraries_differ and not allow_stale:
            raise SystemExit(f"{package}: the installed app isn't the one built at {built_path}: "
                             f"{'; '.join(problems)}. Install it again, or pass --allow-stale.")
        elif problems:
            print(f"apk {name}: WARNING: not the APK built at {built_path}: {'; '.join(problems)}")
    return apks


def dexopt_states(output):
    """Each package's compilation states from `dumpsys package dexopt`: one per APK and
    instruction set, as {"path", "isa", "status", "reason"}."""
    states = {}
    package = path = None
    for line in output.splitlines():
        stripped = line.strip()
        header = re.fullmatch(r"\[([\w.]+)\]", stripped)
        if header:
            package, path = header.group(1), None
            states.setdefault(package, [])
            continue
        if package is None:
            continue
        if stripped.startswith("path:"):
            path = stripped[len("path:"):].strip()
            continue
        state = re.match(r"([\w-]+): \[status=([^\]]+)\] \[reason=([^\]]+)\]", stripped)
        if state:
            states[package].append({"path": path, "isa": state.group(1),
                                    "status": state.group(2), "reason": state.group(3)})
    return states


def log_dexopt(when):
    """The apps' compilation states (`dumpsys package dexopt`), printed and returned."""
    states = dexopt_states(shell("dumpsys package dexopt"))
    apps = {name: states.get(package, []) for name, package in APPS.items()}
    for name, app_states in apps.items():
        print(f"dexopt {when} {name}: " + (", ".join(
            f"{state['isa']} {state['status']} ({state['reason']})" for state in app_states)
            or "not found"))
    return apps


def train(package):
    """Runs the app's screens from a reset, so ART's profile saver records what they run."""
    shell(f"cmd package compile --reset {package}")
    for screen in ("feed", "big", "feed_animated"):
        stop(package)
        time.sleep(1.0)
        start(package, screen)
        time.sleep(2)
        if screen == "feed":
            for flick in range(6):
                swipe(up=flick % 4 != 3)
                time.sleep(1.0)
        time.sleep(3)
    shell("input keyevent KEYCODE_HOME")
    # The profile saver writes the profile a few seconds after the app goes to the background.
    time.sleep(12)
    stop(package)


def compile_apps(mode):
    for package in APPS.values():
        if mode == "speed-profile":
            train(package)
        shell(f"cmd package compile -m {mode} -f {package}")


def meminfo(package):
    output = shell(f"dumpsys meminfo {package}")
    values = {}
    summary = output.split("App Summary", 1)[-1]
    for name in ["Java Heap", "Native Heap", "Code", "Stack", "Graphics", "Private Other", "System"]:
        match = re.search(rf"{name}:\s+(\d+)", summary)
        if match:
            values[name] = int(match.group(1))
    match = re.search(r"TOTAL PSS:\s+(\d+)", summary)
    if match:
        values["Total PSS"] = int(match.group(1))
    match = re.search(r"TOTAL RSS:\s+(\d+)", summary)
    if match:
        values["Total RSS"] = int(match.group(1))
    return values


def gfxinfo(package, reset=False):
    output = shell(f"dumpsys gfxinfo {package}" + (" reset" if reset else ""))
    values = {}
    for key, pattern in [
        ("frames", r"Total frames rendered: (\d+)"),
        ("janky", r"Janky frames: (\d+)"),
        ("p50", r"50th percentile: (\d+)ms"),
        ("p90", r"90th percentile: (\d+)ms"),
        ("p95", r"95th percentile: (\d+)ms"),
        ("p99", r"99th percentile: (\d+)ms"),
        ("gpu_p50", r"50th gpu percentile: (\d+)ms"),
        ("gpu_p90", r"90th gpu percentile: (\d+)ms"),
        ("gpu_p99", r"99th gpu percentile: (\d+)ms"),
    ]:
        match = re.search(pattern, output)
        if match:
            values[key] = int(match.group(1))
    return values


TRACE_CONFIG = """
buffers {{ size_kb: 65536 fill_policy: RING_BUFFER }}
data_sources {{ config {{ name: "android.surfaceflinger.frametimeline" }} }}
data_sources {{ config {{ name: "linux.process_stats" process_stats_config {{ scan_all_processes_on_start: true }} }} }}
data_sources {{ config {{ name: "linux.ftrace" ftrace_config {{
    ftrace_events: "sched/sched_switch"
    ftrace_events: "sched/sched_wakeup"
    ftrace_events: "task/task_newtask"
    ftrace_events: "task/task_rename"
    ftrace_events: "power/cpu_frequency"
    ftrace_events: "power/cpu_idle"
    atrace_categories: "view"
    atrace_categories: "gfx"
    atrace_categories: "am"
    atrace_categories: "wm"
    atrace_categories: "dalvik"
    atrace_apps: "{app}"
}} }} }}
duration_ms: {duration}
"""


def start_trace(package, duration_ms, name):
    config = TRACE_CONFIG.format(app=package, duration=duration_ms)
    path = f"/data/misc/perfetto-traces/{name}.perfetto-trace"
    process = subprocess.Popen(
        ["adb", "shell", f"perfetto --txt -c - -o {path}"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    process.stdin.write(config)
    process.stdin.close()
    time.sleep(1.5)
    return process, path


def finish_trace(process, path, out_dir):
    process.wait(timeout=120)
    local = os.path.join(out_dir, os.path.basename(path))
    adb("pull", path, local)
    return local


def query(trace_processor, trace, sql):
    result = subprocess.run(
        [trace_processor, "--query-string", sql, trace], capture_output=True, text=True
    )
    rows = []
    lines = [line for line in result.stdout.splitlines() if line.strip()]
    if not lines:
        return rows
    header = [name.strip('"') for name in lines[0].split(",")]
    for line in lines[1:]:
        cells = [cell.strip('"') for cell in line.split(",")]
        rows.append(dict(zip(header, cells)))
    return rows


def percentile(values, fraction):
    if not values:
        return None
    ordered = sorted(values)
    index = min(len(ordered) - 1, int(round(fraction * (len(ordered) - 1))))
    return ordered[index]


def cpu_capacities():
    """Each CPU's capacity (the fastest core's is 1024) and its highest frequency, in kHz."""
    output = shell(
        "for c in /sys/devices/system/cpu/cpu[0-9]*; do echo ${c##*cpu} "
        "$(cat $c/cpu_capacity) $(cat $c/cpufreq/cpuinfo_max_freq); done"
    )
    capacities = {}
    for line in output.split("\n"):
        parts = line.split()
        if len(parts) == 3 and all(part.isdigit() for part in parts):
            capacities[int(parts[0])] = (int(parts[1]), int(parts[2]))
    return capacities


def work_ms(trace_processor, trace, package, capacities, since=0):
    """The main thread's and RenderThread's work as milliseconds on the fastest core at its
    highest frequency: each running slice's time scaled by its core's frequency and capacity.
    Unlike time, it doesn't grow when the scheduler moves light work to slower cores. From
    `since` (a trace timestamp) on."""
    if not capacities:
        return None
    scale = " ".join(
        f"WHEN {cpu} THEN {capacity / 1024.0} / {max_freq}"
        for cpu, (capacity, max_freq) in capacities.items()
    )
    rows = query(
        trace_processor,
        trace,
        f"""
        INCLUDE PERFETTO MODULE linux.cpu.frequency;
        CREATE PERFETTO TABLE frequencies AS
          SELECT ts, dur, cpu, freq FROM cpu_frequency_counters WHERE dur > 0;
        CREATE PERFETTO TABLE running AS
          SELECT s.ts, s.dur, s.cpu,
                 CASE WHEN t.name = 'RenderThread' THEN 'render' ELSE 'main' END AS thread
          FROM sched s JOIN thread t USING (utid) JOIN process p USING (upid)
          WHERE p.name = '{package}' AND s.dur > 0 AND s.ts >= {since}
            AND (t.tid = p.pid OR t.name = 'RenderThread');
        CREATE VIRTUAL TABLE joined USING SPAN_JOIN(running PARTITIONED cpu,
          frequencies PARTITIONED cpu);
        SELECT thread, SUM(dur * freq * (CASE cpu {scale} END)) / 1e6 AS work
        FROM joined GROUP BY thread
        """,
    )
    work = {"main": 0.0, "render": 0.0}
    for row in rows:
        try:
            work[row["thread"]] = float(row["work"])
        except (KeyError, ValueError):
            pass
    return work


# FrameTimeline's jank types (a frame may have several, comma-separated), by whose they are.
APP_JANK = {"App Deadline Missed", "App Resynced Jitter"}
COMPOSITOR_JANK = {"SurfaceFlinger CPU Deadline Missed", "SurfaceFlinger GPU Deadline Missed",
                   "SurfaceFlinger Scheduling", "SurfaceFlinger Stuffing", "Prediction Error",
                   "Display HAL"}


def jank_side(jank_type):
    """Whose a frame's jank is: "app" (the app missed its deadline), "compositor" (SurfaceFlinger
    or the display: its deadlines, its predictions, its scheduling), "other" (unknown, dropped),
    or None for no jank. Buffer stuffing alone (the app's frames queued ahead, presented late
    but on cadence) isn't counted as jank, as before."""
    kinds = {kind.strip() for kind in jank_type.split(",")} - {"None", "Buffer Stuffing", ""}
    if not kinds:
        return None
    if kinds & APP_JANK:
        return "app"
    if kinds & COMPOSITOR_JANK:
        return "compositor"
    return "other"


# Present types of a frame shown at another vsync than expected, or not at all.
VISIBLY_LATE = ("Late Present", "Early Present", "Dropped Frame")


def measured_since(input_timestamps, gap_ns=500_000_000):
    """Where the measured gestures start: the first input event after the first pause of more
    than `gap_ns` (the warm-up swipe's events come before it), or 0 (the trace's start)
    without one."""
    ordered = sorted(input_timestamps)
    for before, after in zip(ordered, ordered[1:]):
        if after - before > gap_ns:
            return after
    return 0


def input_timestamps(trace_processor, trace, package):
    """When the app's main thread got each input event."""
    rows = query(
        trace_processor,
        trace,
        f"""
        SELECT s.ts AS ts FROM slice s
        JOIN thread_track tt ON s.track_id = tt.id
        JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND t.tid = p.pid AND s.name LIKE 'deliverInputEvent%'
        """,
    )
    return [int(row["ts"]) for row in rows if row.get("ts", "").isdigit()]


def count(trace_processor, trace, sql):
    """The `n` of a query's one row (0 if none)."""
    rows = query(trace_processor, trace, sql)
    value = rows[0].get("n", "") if rows else ""
    return int(value) if value.isdigit() else 0


# A doFrame starting this long after its vsync was delayed (by idle-time work, usually): on
# the Pixel 8 Pro they start about 1.1 ms after it.
LATE_DO_FRAME_NS = 2_000_000


def frame_stats(trace_processor, trace, package, capacities=None, since=0):
    """FrameTimeline jank and the app's main thread and RenderThread time per frame, from
    `since` (a trace timestamp: after a warm-up) on."""
    timeline = query(
        trace_processor,
        trace,
        f"""
        SELECT a.dur AS dur, a.jank_type AS jank, a.present_type AS present
        FROM actual_frame_timeline_slice a JOIN process p USING (upid)
        WHERE p.name = '{package}' AND a.ts >= {since}
        """,
    )
    jank_kinds = {}
    jank_sides = {"app": 0, "compositor": 0, "other": 0}
    for row in timeline:
        side = jank_side(row["jank"])
        if side:
            jank_kinds[row["jank"]] = jank_kinds.get(row["jank"], 0) + 1
            jank_sides[side] += 1
    janky = sum(jank_kinds.values())
    visibly_late = sum(row.get("present") in VISIBLY_LATE for row in timeline)

    def durations(thread_condition, name_condition):
        rows = query(
            trace_processor,
            trace,
            f"""
            SELECT s.dur AS dur FROM slice s
            JOIN thread_track tt ON s.track_id = tt.id
            JOIN thread t USING (utid) JOIN process p USING (upid)
            WHERE p.name = '{package}' AND {thread_condition} AND {name_condition}
              AND s.ts >= {since}
            """,
        )
        return [int(row["dur"]) / 1e6 for row in rows if row.get("dur", "").isdigit()]

    main = durations("t.tid = p.pid", "s.name LIKE 'Choreographer#doFrame%'")
    render = durations("t.name = 'RenderThread'", "s.name LIKE 'DrawFrame%'")
    # CPU time of the main thread and RenderThread while traced.
    cpu = query(
        trace_processor,
        trace,
        f"""
        SELECT t.name AS thread, SUM(ts.dur) AS running FROM thread_state ts
        JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND ts.state = 'Running' AND ts.ts >= {since}
          AND (t.tid = p.pid OR t.name = 'RenderThread')
        GROUP BY t.utid
        """,
    )
    cpu_ms = {"main": 0.0, "render": 0.0}
    for row in cpu:
        if row.get("running", "").isdigit():
            kind = "render" if row["thread"] == "RenderThread" else "main"
            cpu_ms[kind] += int(row["running"]) / 1e6
    app_slices = f"""
        FROM slice s JOIN thread_track tt ON s.track_id = tt.id
        JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND s.ts >= {since}"""
    # doFrames that started late after their vsync (the expected timeline's start, by vsync id).
    late_do_frames = count(trace_processor, trace, f"""
        SELECT COUNT(*) AS n {app_slices} AND t.tid = p.pid
          AND s.name GLOB 'Choreographer#doFrame [0-9]*'
          AND s.ts - (SELECT MIN(e.ts) FROM expected_frame_timeline_slice e
                      WHERE e.upid = p.upid AND e.name = SUBSTR(s.name, 23))
              > {LATE_DO_FRAME_NS}
        """)
    prefetches = count(trace_processor, trace, f"""
        SELECT COUNT(*) AS n {app_slices} AND t.tid = p.pid AND s.name = 'compose-rs: prefetch'
        """)
    # Android 16's Choreographer after a display stall: frame times an interval back until the
    # app goes idle (an async track of the app's), and "buffer stuffed" slices.
    recovery = count(trace_processor, trace, f"""
        SELECT COUNT(s.id) AS n FROM process_track pt JOIN process p USING (upid)
        JOIN slice s ON s.track_id = pt.id
        WHERE p.name = '{package}' AND pt.name = 'Buffer stuffing recovery' AND s.ts >= {since}
        """)
    stuffed = count(trace_processor, trace, f"""
        SELECT COUNT(*) AS n {app_slices} AND s.name = 'buffer stuffed'
        """)
    return {
        "since": since,
        "frames": len(timeline),
        "janky_frames": janky,
        "jank_kinds": jank_kinds,
        "jank_sides": jank_sides,
        "visibly_late_frames": visibly_late,
        "late_do_frames": late_do_frames,
        "prefetches": prefetches,
        "buffer_stuffing": {"recovery_slices": recovery, "buffer_stuffed": stuffed,
                            "occurred": recovery > 0 or stuffed > 0},
        "main_thread_frame_ms": {
            "count": len(main),
            "p50": percentile(main, 0.5),
            "p90": percentile(main, 0.9),
            "p99": percentile(main, 0.99),
            "max": max(main) if main else None,
        },
        "render_thread_frame_ms": {
            "count": len(render),
            "p50": percentile(render, 0.5),
            "p90": percentile(render, 0.9),
            "p99": percentile(render, 0.99),
        },
        "cpu_ms": cpu_ms,
        "work_ms": work_ms(trace_processor, trace, package, capacities, since),
    }


def first_frame(trace_processor, trace, package):
    """The first frames' main-thread slices after the activity started, and compose-rs's."""
    rows = query(
        trace_processor,
        trace,
        f"""
        SELECT s.name AS name, s.dur AS dur FROM slice s
        JOIN thread_track tt ON s.track_id = tt.id
        JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND t.tid = p.pid
          AND (s.name LIKE 'Choreographer#doFrame%' OR s.name IN
               ('bindApplication', 'activityStart', 'activityResume', 'Inflate',
                'Compose:recompose', 'compose-rs: frame', 'compose-rs: recompose',
                'compose-rs: layout', 'compose-rs: draw', 'compose-rs: serialize',
                'compose-rs: replay', 'compose-rs: text layout', 'traversal', 'draw',
                'Record View#draw()', 'measure', 'layout'))
        ORDER BY s.ts
        """,
    )
    firsts = {}
    totals = {}
    for row in rows:
        if not row.get("dur", "").isdigit():
            continue
        name = row["name"]
        if name.startswith("Choreographer#doFrame"):
            name = "Choreographer#doFrame"
        dur = int(row["dur"]) / 1e6
        firsts.setdefault(name, dur)
        totals[name] = totals.get(name, 0.0) + dur
    return {"first": firsts, "total_during_trace": totals}


def swipe(up):
    if up:
        shell("input swipe 504 1800 504 500 120")
    else:
        shell("input swipe 504 500 504 1800 120")


def scroll_run(trace_processor, out_dir, package, name, run, capacities):
    """One traced run of 10 flings through the feed, after a warm-up swipe: the display, idle
    since the app started, resyncs its vsync on the first input, which FrameTimeline counts as
    jank no app causes. The warm-up's frames are left out of the stats (from the first input
    after it), and gfxinfo is reset after it. Returns the memory after start, and the stats."""
    fresh(package)
    time.sleep(1.0)
    start(package, "feed")
    time.sleep(5)
    after_start = meminfo(package)
    process, path = start_trace(package, 18000, f"scroll-{name}-{run}")
    swipe(up=True)
    time.sleep(1.2)
    gfxinfo(package, reset=True)
    for flick in range(10):
        swipe(up=flick % 4 != 3)
        time.sleep(1.2)
    trace = finish_trace(process, path, out_dir)
    since = measured_since(input_timestamps(trace_processor, trace, package))
    stats = {"gfxinfo": gfxinfo(package),
             "trace": frame_stats(trace_processor, trace, package, capacities, since)}
    return after_start, stats


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--trace-processor", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--compile", choices=["speed", "speed-profile", "verify"],
                        default="speed", help="the apps' ART compilation filter (see above)")
    parser.add_argument("--allow-stale", action="store_true",
                        help="measure even if the installed APKs aren't the ones built here")
    arguments = parser.parse_args()
    os.makedirs(arguments.out, exist_ok=True)
    tp = arguments.trace_processor
    capacities = cpu_capacities()
    report = {"device": shell("getprop ro.product.model").strip(),
              "android": shell("getprop ro.build.version.release").strip(),
              "abi": shell("getprop ro.product.cpu.abi").strip(),
              "compile": arguments.compile}
    report["apks"] = check_apks(arguments.out, report["abi"], arguments.allow_stale)

    shell("input keyevent KEYCODE_WAKEUP")
    shell("wm dismiss-keyguard")
    compile_apps(arguments.compile)
    report["dexopt"] = {"start": log_dexopt("start")}

    # Startup: cold starts, interleaved.
    startup = {name: {"feed": [], "big": []} for name in APPS}
    for _ in range(arguments.runs):
        for screen in ("feed", "big"):
            for name, package in APPS.items():
                startup[name][screen].append(cold_start(package, screen))
    report["startup_ms"] = {
        name: {screen: {"runs": runs, "median": statistics.median(runs), "min": min(runs),
                        "max": max(runs)} for screen, runs in screens.items()}
        for name, screens in startup.items()
    }
    print("startup", json.dumps(report["startup_ms"], indent=1))

    # The first frame of each screen, traced on a cold start.
    first = {}
    for name, package in APPS.items():
        first[name] = {}
        for screen in ("feed", "big"):
            fresh(package)
            time.sleep(1.0)
            process, path = start_trace(package, 5000, f"start-{name}-{screen}")
            start(package, screen, wait=False)
            trace = finish_trace(process, path, arguments.out)
            first[name][screen] = first_frame(tp, trace, package)
    report["first_frame"] = first
    print("first frame", json.dumps(first, indent=1))

    # Memory, and scrolling jank.
    memory = {name: {"after_start": [], "after_scroll": []} for name in APPS}
    scroll = {name: [] for name in APPS}
    for run in range(3):
        for name, package in APPS.items():
            after_start, stats = scroll_run(tp, arguments.out, package, name, run, capacities)
            memory[name]["after_start"].append(after_start)
            scroll[name].append(stats)
            memory[name]["after_scroll"].append(meminfo(package))
            print("scroll", name, run, json.dumps(stats))
    report["memory_kb"] = memory
    report["scroll"] = scroll

    # The animated header, untouched.
    animation = {name: [] for name in APPS}
    for run in range(3):
        for name, package in APPS.items():
            fresh(package)
            time.sleep(1.0)
            start(package, "feed_animated")
            time.sleep(3)
            gfxinfo(package, reset=True)
            process, path = start_trace(package, 10000, f"animation-{name}-{run}")
            trace = finish_trace(process, path, arguments.out)
            stats = {"gfxinfo": gfxinfo(package), "trace": frame_stats(tp, trace, package, capacities)}
            animation[name].append(stats)
            print("animation", name, run, json.dumps(stats))
    report["animation"] = animation

    for package in APPS.values():
        stop(package)
    # A background dexopt job can recompile an app during a run.
    report["dexopt"]["end"] = log_dexopt("end")
    with open(os.path.join(arguments.out, "report.json"), "w") as file:
        json.dump(report, file, indent=1)
    print("wrote", os.path.join(arguments.out, "report.json"))


if __name__ == "__main__":
    main()
