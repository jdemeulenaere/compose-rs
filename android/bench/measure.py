#!/usr/bin/env python3
"""Measures the benchmark apps (bench-compose: Kotlin Compose; bench-rs: compose-rs) on a
connected device, interleaving the two apps run by run, and writes a JSON report.

    measure.py --trace-processor PATH --out DIR [--runs N] [--compile MODE] [--allow-stale]
               [--scenarios NAME,NAME,...]

Before measuring, the installed APKs are checked against the ones built in this checkout
(android/bench-*/build/outputs/apk/release/): the device ABI's native libraries' SHA-1s must
match (an install from Android Studio, or as splits, holds only those), or the run stops
(--allow-stale measures anyway). The report records each APK's size and SHA-1 and its
libraries', and each app's ART compilation state (`dumpsys package dexopt`) at the start and the
end of the run.

--compile (`cmd package compile -m MODE -f`, before measuring):
  speed          (default) everything compiled ahead of time; docs/12's rounds 1 to 3 used it
  speed-profile  each app is stopped and reset, given the Baseline Profile its APK ships
                 (through androidx.profileinstaller's receiver, as Macrobenchmark installs one),
                 then compiled from it: what a Play install compiles at install, with an app
                 image (D362). bench-compose's is Compose's libraries' and its own; bench-rs's is
                 the compose-rs library's and its own (D360)
  verify         no ahead-of-time code: a fresh install with no profile
The run stops if a step fails: a reset, a profile not installed, or an app whose compilation
state isn't the filter asked for.

Scenarios (docs/12-android-performance.md):
  startup      cold starts of the feed and of the big screen (`am start -W`: TotalTime), and a
               Perfetto trace of a cold start of each for the first frame's breakdown
  memory       PSS and its parts (`dumpsys meminfo`) after start, and after scrolling
  scroll       10 flings through the feed, after a warm-up swipe left out: frames
               (`dumpsys gfxinfo`) and jank (FrameTimeline: the app's or the compositor's, and
               frames presented at another vsync than expected), doFrames starting late,
               compose-rs's prefetches, whether Choreographer recovered from buffer stuffing, and
               the RenderThread's boost picture: the HWUI performance-hint session's boost
               minimum (ADPF's uclamp.min), its frames' durations by boost, and its running
               time's share on the little cores
  animation    10 s of the animated header, no input: the same
  stress_startup    cold starts of the stress list (docs/15), and a traced one
  stress_scroll     memory after start, 10 flings through the stress list as `scroll`, memory
                    after them
  stress_animation  10 s of the stress animation, as `animation`, with memory before and after,
                    and HWUI's GPU caches (`dumpsys gfxinfo`) at both points

--scenarios picks some of them (all by default), comma-separated; the report holds those run.
Each traced run also lists its slowest frames on each thread with their longest slices
(`slowest_frames`).

Before each run, our other apps (`rs.compose.*`) are force-stopped (D343).
"""

import argparse
import bisect
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


# androidx.profileinstaller's `ProfileInstaller.RESULT_INSTALL_SUCCESS`.
PROFILE_INSTALLED = 1


def broadcast_result(output):
    """The result code in `am broadcast`'s output, or None (no `result=`)."""
    match = re.search(r"Broadcast completed: result=(-?\d+)", output)
    return int(match.group(1)) if match else None


def install_baseline_profile(package):
    """Has the app's androidx.profileinstaller write the Baseline Profile its APK ships
    (`assets/dexopt/baseline.prof`) where ART compiles from, as Macrobenchmark does. Stops the
    run if it didn't: an app without profileinstaller would be compiled from no profile."""
    output = shell(f"am broadcast -a androidx.profileinstaller.action.INSTALL_PROFILE "
                   f"{package}/androidx.profileinstaller.ProfileInstallReceiver")
    stop(package)
    result = broadcast_result(output)
    if result != PROFILE_INSTALLED:
        raise SystemExit(f"{package}: its Baseline Profile wasn't installed (profileinstaller's "
                         f"result {result}; {PROFILE_INSTALLED} is success): {output.strip()}")


def compile_apps(mode):
    for package in APPS.values():
        if mode == "speed-profile":
            # Without the profiles of earlier runs or compilations: the APK's only. Stopped
            # first, so a running process's profile saver can't write after the reset.
            stop(package)
            output = shell(f"cmd package compile --reset {package}")
            # "Success" (Android 16, ART Service); some versions print the dexopt result.
            if "Success" not in output and "PERFORMED" not in output:
                raise SystemExit(f"{package}: `cmd package compile --reset` failed: {output.strip()}")
            install_baseline_profile(package)
        shell(f"cmd package compile -m {mode} -f {package}")


def check_compiled(apps, mode):
    """Stops the run unless each app's states (`log_dexopt`'s) are all `mode`: a profile ART
    found empty compiles to `verify`, say."""
    for name, states in apps.items():
        statuses = sorted({state["status"] for state in states})
        if statuses != [mode]:
            raise SystemExit(f"{name}: compiled as {statuses or 'nothing'}, not {mode}")


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


def gpu_caches(package):
    """HWUI's GPU caches from `dumpsys gfxinfo`: [MB, entries] by category and kind
    ("Scratch/Surface", say), and the total and its purgeable part (MB)."""
    text = shell(f"dumpsys gfxinfo {package}")
    section = text.split("GPU Caches:", 1)[-1].split("Total GPU memory usage", 1)
    out = {}
    category = None
    for line in section[0].splitlines():
        line = line.strip()
        if line.endswith(":") and not re.search(r"\d", line):
            category = line[:-1]
            continue
        match = re.match(r"(.+?): ([\d.]+) (\w+) \((\d+) entr", line)
        if match and category:
            size = float(match.group(2)) * {"bytes": 1 / 2**20, "KB": 1 / 1024,
                                            "MB": 1.0}.get(match.group(3), 0.0)
            out[f"{category}/{match.group(1)}"] = [round(size, 2), int(match.group(4))]
    if len(section) > 1:
        match = re.search(r"([\d.]+) MB \(([\d.]+) MB is purgeable\)", section[1])
        if match:
            out["total"] = float(match.group(1))
            out["purgeable"] = float(match.group(2))
    return out


TRACE_CONFIG = """
buffers {{ size_kb: {buffer_kb} fill_policy: RING_BUFFER }}
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


# The trace buffer: enough for the feed's scenarios. The stress screens' traces hold several
# times more (HWUI traces each of its operations: 140 cells' layers each frame), and a ring
# buffer that wraps loses the process table scanned at the start, and with it every frame.
TRACE_BUFFER_KB = 65536
STRESS_TRACE_BUFFER_KB = 393216


def start_trace(package, duration_ms, name, buffer_kb=TRACE_BUFFER_KB):
    config = TRACE_CONFIG.format(app=package, duration=duration_ms, buffer_kb=buffer_kb)
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
    # A failed query (a typo, a table a new trace processor renamed) must not read as no rows:
    # the counts below would show it as a good result.
    if result.returncode != 0:
        raise RuntimeError(f"trace processor failed on {trace}: {result.stderr[-2000:]}")
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


# The RenderThread's frames by the HWUI performance-hint session's boost (its uclamp.min, out of
# 1024) when they started: (name, lowest, highest excluded or None).
BOOST_BUCKETS = (("unboosted", 0, 100), ("100-199", 100, 200), ("200+", 200, None))


def little_core_share(running_by_cpu, capacities):
    """The share of running time (ns by CPU) on the little cores: those of the lowest capacity
    (`cpu_capacities()`). None without capacities, with cores all alike, or without running
    time."""
    if not capacities or not running_by_cpu:
        return None
    lowest = min(capacity for capacity, _ in capacities.values())
    if all(capacity == lowest for capacity, _ in capacities.values()):
        return None
    total = sum(running_by_cpu.values())
    if total <= 0:
        return None
    little = sum(running for cpu, running in running_by_cpu.items()
                 if cpu in capacities and capacities[cpu][0] == lowest)
    return little / total


def boost_picture(frames, samples, running_by_cpu, capacities):
    """How much Android's performance hints (ADPF) boosted the RenderThread, and where it ran.

    `frames`: its DrawFrame slices as (ts, dur) in ns. `samples`: the app's HWUI session's
    boost minimum (the `adpf.<pid>-…-HWUI-…-min` counter: the uclamp.min ADPF asks for, out of
    1024) as (ts, value). `running_by_cpu`: its running time (ns) by CPU.

    Returns the boost minimum's time-weighted mean over the frames' span, the share of frames
    started at a minimum under 100 (unboosted), each bucket's share of frames and duration
    percentiles (ms), and the share of running time on little cores. The boost's fields are
    None in a trace without the counter; frames before its first value aren't bucketed
    (`frames_before_counter`)."""
    picture = {"counter": bool(samples), "time_weighted_min": None,
               "unboosted_frame_share": None, "frames_before_counter": 0, "buckets": None,
               "little_core_share": little_core_share(running_by_cpu, capacities)}
    if not samples or not frames:
        return picture
    samples = sorted(samples)
    times = [ts for ts, _ in samples]
    frames = sorted(frames)
    start = frames[0][0]
    end = max(ts + dur for ts, dur in frames)
    weighted = covered = 0
    for index, (ts, value) in enumerate(samples):
        until = samples[index + 1][0] if index + 1 < len(samples) else end
        span = min(until, end) - max(ts, start)
        if span > 0:
            weighted += value * span
            covered += span
    picture["time_weighted_min"] = weighted / covered if covered else None
    durations = {name: [] for name, _, _ in BOOST_BUCKETS}
    for ts, dur in frames:
        index = bisect.bisect_right(times, ts) - 1
        if index < 0:
            picture["frames_before_counter"] += 1
            continue
        value = samples[index][1]
        for name, lowest, highest in BOOST_BUCKETS:
            if value >= lowest and (highest is None or value < highest):
                durations[name].append(dur / 1e6)
                break
    bucketed = sum(len(values) for values in durations.values())
    if not bucketed:
        return picture
    picture["unboosted_frame_share"] = len(durations["unboosted"]) / bucketed
    picture["buckets"] = {
        name: {"frames": len(values), "share": len(values) / bucketed,
               "p50": percentile(values, 0.5), "p90": percentile(values, 0.9),
               "p99": percentile(values, 0.99)}
        for name, values in durations.items()}
    return picture


def boost_stats(trace_processor, trace, package, capacities, since=0):
    """`boost_picture` of the app's RenderThread, from `since` (a trace timestamp) on."""
    frames = query(trace_processor, trace, f"""
        SELECT s.ts AS ts, s.dur AS dur FROM slice s
        JOIN thread_track tt ON s.track_id = tt.id
        JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND t.name = 'RenderThread' AND s.name LIKE 'DrawFrame%'
          AND s.ts >= {since} AND s.dur > 0
        """)
    # The power HAL's counters per hint session, named `adpf.<pid>-<uid>-<session>-<name>-…`:
    # HWUI's for the app's process (each value holds until the next).
    samples = query(trace_processor, trace, f"""
        SELECT c.ts AS ts, c.value AS value FROM counter c
        JOIN counter_track ct ON c.track_id = ct.id
        JOIN process p ON ct.name GLOB 'adpf.' || p.pid || '-*-HWUI-*-min'
        WHERE p.name = '{package}'
        """)
    running = query(trace_processor, trace, f"""
        SELECT s.cpu AS cpu, SUM(s.dur) AS running FROM sched s
        JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND t.name = 'RenderThread' AND s.dur > 0 AND s.ts >= {since}
        GROUP BY s.cpu
        """)

    def number(text):
        try:
            return float(text)
        except (TypeError, ValueError):
            return None

    frame_list = [(int(row["ts"]), int(row["dur"])) for row in frames
                  if row.get("ts", "").isdigit() and row.get("dur", "").isdigit()]
    sample_list = [(int(row["ts"]), number(row.get("value"))) for row in samples
                   if row.get("ts", "").isdigit() and number(row.get("value")) is not None]
    running_by_cpu = {int(row["cpu"]): int(row["running"]) for row in running
                      if row.get("cpu", "").isdigit() and row.get("running", "").isdigit()}
    return boost_picture(frame_list, sample_list, running_by_cpu, capacities)


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
# Work on the main thread ending this long after a vsync delays its doFrame.
WORK_PAST_VSYNC_NS = 1_200_000


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
    # doFrames that started late after their vsync (the expected timeline's start, by vsync
    # id): most because the main thread woke late; some because work on it (idle-time work,
    # input) ran past the vsync by more than a normal start's latency. A doFrame that drew
    # nothing has no expected timeline (about 1.5 % of them): not counted.
    late = f"""{app_slices} AND t.tid = p.pid
          AND s.name GLOB 'Choreographer#doFrame [0-9]*'
          AND s.ts - (SELECT MIN(e.ts) FROM expected_frame_timeline_slice e
                      WHERE e.upid = p.upid AND e.name = SUBSTR(s.name, 23))
              > {LATE_DO_FRAME_NS}"""
    late_do_frames = count(trace_processor, trace, f"SELECT COUNT(*) AS n {late}")
    late_after_work = count(trace_processor, trace, f"""
        SELECT COUNT(*) AS n {late}
          AND EXISTS (SELECT 1 FROM slice w WHERE w.track_id = s.track_id AND w.depth = 0
                      AND w.name NOT GLOB 'Choreographer#*' AND w.ts < s.ts
                      AND w.ts + w.dur > (SELECT MIN(e.ts) FROM expected_frame_timeline_slice e
                                          WHERE e.upid = p.upid
                                            AND e.name = SUBSTR(s.name, 23))
                                         + {WORK_PAST_VSYNC_NS})
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
        "late_do_frames_after_work": late_after_work,
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
        "render_boost": boost_stats(trace_processor, trace, package, capacities, since),
        "slowest_frames": slowest_frames(trace_processor, trace, package, since),
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


# The scenarios, in the order they run (docs/12, docs/15).
SCENARIOS = ("startup", "scroll", "animation", "stress_startup", "stress_scroll",
             "stress_animation")


def parse_scenarios(text):
    """The scenarios `--scenarios` names (comma-separated; empty or "all": all of them), in
    SCENARIOS' order. Stops the run on an unknown name."""
    names = [name.strip() for name in (text or "").split(",") if name.strip()]
    if not names or names == ["all"]:
        return list(SCENARIOS)
    unknown = sorted(set(names) - set(SCENARIOS))
    if unknown:
        raise SystemExit(f"unknown scenarios {unknown}; known: {', '.join(SCENARIOS)}")
    return [name for name in SCENARIOS if name in names]


# How many of a run's slowest frames, and of each one's longest slices, `slowest_frames` lists.
SLOWEST_FRAMES = 5
LONGEST_SLICES = 8


def slowest_frames(trace_processor, trace, package, since=0):
    """The run's slowest `Choreographer#doFrame`s (main thread) and `DrawFrame`s (RenderThread),
    each with its start, its duration (ms) and its longest slices inside it (any depth): where
    the time of the frames that set p99 goes."""
    result = {}
    for thread, thread_condition, name_condition in (
            ("main", "t.tid = p.pid", "s.name LIKE 'Choreographer#doFrame%'"),
            ("render", "t.name = 'RenderThread'", "s.name LIKE 'DrawFrame%'")):
        frames = query(trace_processor, trace, f"""
            SELECT s.id AS id, s.ts AS ts, s.dur AS dur FROM slice s
            JOIN thread_track tt ON s.track_id = tt.id
            JOIN thread t USING (utid) JOIN process p USING (upid)
            WHERE p.name = '{package}' AND {thread_condition} AND {name_condition}
              AND s.ts >= {since} AND s.dur > 0
            ORDER BY s.dur DESC LIMIT {SLOWEST_FRAMES}
            """)
        listed = []
        for frame in frames:
            if not frame.get("id", "").isdigit() or not frame.get("dur", "").isdigit():
                continue
            inside = query(trace_processor, trace, f"""
                SELECT d.name AS name, d.dur AS dur, d.depth AS depth
                FROM descendant_slice({frame['id']}) d
                WHERE d.dur > 0 ORDER BY d.dur DESC LIMIT {LONGEST_SLICES}
                """)
            listed.append({
                "ts": int(frame["ts"]), "ms": int(frame["dur"]) / 1e6,
                "slices": [{"name": row["name"], "ms": int(row["dur"]) / 1e6,
                            "depth": int(row["depth"])}
                           for row in inside if row.get("dur", "").isdigit()
                           and row.get("depth", "").isdigit()]})
        result[thread] = listed
    return result


def swipe(up):
    if up:
        shell("input swipe 504 1800 504 500 120")
    else:
        shell("input swipe 504 500 504 1800 120")


# Traced runs tried again when their trace holds no frame of the app's (a trace that missed the
# app's process, say), at most this many times in all; then the run stops.
TRACE_ATTEMPTS = 3


def traced(attempt, what):
    """`attempt()`'s result (`(…, stats)` or `stats`, whose "trace" has FrameTimeline's
    "frames"), tried again while its trace counted no frame: an empty run would read as zero work
    and no percentiles, and pull the medians down."""
    for _ in range(TRACE_ATTEMPTS):
        result = attempt()
        stats = result[-1] if isinstance(result, tuple) else result
        if stats["trace"]["frames"] > 0:
            return result
        print(f"{what}: the trace has no frame of the app's; again")
    raise SystemExit(f"{what}: no frame traced in {TRACE_ATTEMPTS} attempts")


def scroll_run(trace_processor, out_dir, package, name, run, capacities, screen="feed",
               prefix="scroll", buffer_kb=TRACE_BUFFER_KB):
    """One traced run of 10 flings through the feed (or `screen`), after a warm-up swipe: the
    display, idle since the app started, resyncs its vsync on the first input, which
    FrameTimeline counts as jank no app causes. The warm-up's frames are left out of the stats
    (from the first input after it), and gfxinfo is reset after it. Returns the memory after
    start, and the stats."""
    fresh(package)
    time.sleep(1.0)
    start(package, screen)
    time.sleep(5)
    after_start = meminfo(package)
    process, path = start_trace(package, 18000, f"{prefix}-{name}-{run}", buffer_kb)
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


def startup_scenario(tp, out_dir, runs, screens):
    """Cold starts of each of `screens`, interleaved by app, `runs` times; then one traced cold
    start of each for the first frame's breakdown."""
    startup = {name: {screen: [] for screen in screens} for name in APPS}
    for _ in range(runs):
        for screen in screens:
            for name, package in APPS.items():
                startup[name][screen].append(cold_start(package, screen))
    startup_ms = {
        name: {screen: {"runs": times, "median": statistics.median(times), "min": min(times),
                        "max": max(times)} for screen, times in by_screen.items()}
        for name, by_screen in startup.items()
    }
    print("startup", json.dumps(startup_ms, indent=1))
    first = {}
    for name, package in APPS.items():
        first[name] = {}
        for screen in screens:
            fresh(package)
            time.sleep(1.0)
            process, path = start_trace(package, 5000, f"start-{name}-{screen}")
            start(package, screen, wait=False)
            trace = finish_trace(process, path, out_dir)
            first[name][screen] = first_frame(tp, trace, package)
    print("first frame", json.dumps(first, indent=1))
    return startup_ms, first


def scroll_scenario(tp, out_dir, capacities, screen, prefix, buffer_kb=TRACE_BUFFER_KB):
    """3 traced scroll runs of each app through `screen`, interleaved, with the memory after
    start and after the flings."""
    memory = {name: {"after_start": [], "after_scroll": []} for name in APPS}
    scroll = {name: [] for name in APPS}
    for run in range(3):
        for name, package in APPS.items():
            after_start, stats = traced(
                lambda: scroll_run(tp, out_dir, package, name, run, capacities, screen, prefix,
                                   buffer_kb),
                f"{prefix} {name} {run}")
            memory[name]["after_start"].append(after_start)
            scroll[name].append(stats)
            memory[name]["after_scroll"].append(meminfo(package))
            print(prefix, name, run, json.dumps(stats))
    return memory, scroll


def animation_scenario(tp, out_dir, capacities, screen, prefix, memory=False,
                       buffer_kb=TRACE_BUFFER_KB):
    """3 traced runs of each app showing `screen` for 10 s untouched, interleaved, 3 s after
    it started. With `memory`, each run also has the memory before the trace (`after_start`;
    `dumpsys meminfo` collects garbage, so it's taken before gfxinfo's reset) and after it
    (`after_animation`), and HWUI's GPU caches at both points (`gpu_after_start`,
    `gpu_after_animation`)."""
    animation = {name: [] for name in APPS}
    for run in range(3):
        for name, package in APPS.items():
            def animation_run():
                fresh(package)
                time.sleep(1.0)
                start(package, screen)
                time.sleep(3)
                before = meminfo(package) if memory else None
                gpu_before = gpu_caches(package) if memory else None
                gfxinfo(package, reset=True)
                process, path = start_trace(package, 10000, f"{prefix}-{name}-{run}",
                                            buffer_kb)
                trace = finish_trace(process, path, out_dir)
                stats = {"gfxinfo": gfxinfo(package),
                         "trace": frame_stats(tp, trace, package, capacities)}
                if memory:
                    stats["memory_kb"] = {"after_start": before,
                                          "after_animation": meminfo(package)}
                    stats["gpu_caches_mb"] = {"after_start": gpu_before,
                                              "after_animation": gpu_caches(package)}
                return stats
            stats = traced(animation_run, f"{prefix} {name} {run}")
            animation[name].append(stats)
            print(prefix, name, run, json.dumps(stats))
    return animation


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
    parser.add_argument("--scenarios", default="all",
                        help="the scenarios to run, comma-separated (see above; default all)")
    arguments = parser.parse_args()
    scenarios = parse_scenarios(arguments.scenarios)
    os.makedirs(arguments.out, exist_ok=True)
    tp = arguments.trace_processor
    capacities = cpu_capacities()
    report = {"device": shell("getprop ro.product.model").strip(),
              "android": shell("getprop ro.build.version.release").strip(),
              "abi": shell("getprop ro.product.cpu.abi").strip(),
              "compile": arguments.compile,
              "scenarios": scenarios,
              # Each CPU's capacity and highest frequency (kHz): what "work" and the little
              # cores' share are computed from.
              "cpu_capacities": {str(cpu): list(value) for cpu, value in capacities.items()}}
    report["apks"] = check_apks(arguments.out, report["abi"], arguments.allow_stale)

    shell("input keyevent KEYCODE_WAKEUP")
    shell("wm dismiss-keyguard")
    compile_apps(arguments.compile)
    report["dexopt"] = {"start": log_dexopt("start")}
    check_compiled(report["dexopt"]["start"], arguments.compile)

    if "startup" in scenarios:
        report["startup_ms"], report["first_frame"] = startup_scenario(
            tp, arguments.out, arguments.runs, ("feed", "big"))
    if "scroll" in scenarios:
        report["memory_kb"], report["scroll"] = scroll_scenario(
            tp, arguments.out, capacities, "feed", "scroll")
    if "animation" in scenarios:
        report["animation"] = animation_scenario(
            tp, arguments.out, capacities, "feed_animated", "animation")
    if "stress_startup" in scenarios:
        report["stress_startup_ms"], report["stress_first_frame"] = startup_scenario(
            tp, arguments.out, arguments.runs, ("stress",))
    if "stress_scroll" in scenarios:
        report["stress_memory_kb"], report["stress_scroll"] = scroll_scenario(
            tp, arguments.out, capacities, "stress", "stress-scroll", STRESS_TRACE_BUFFER_KB)
    if "stress_animation" in scenarios:
        report["stress_animation"] = animation_scenario(
            tp, arguments.out, capacities, "stress_animation", "stress-animation", memory=True,
            buffer_kb=STRESS_TRACE_BUFFER_KB)

    for package in APPS.values():
        stop(package)
    # A background dexopt job can recompile an app during a run.
    report["dexopt"]["end"] = log_dexopt("end")
    with open(os.path.join(arguments.out, "report.json"), "w") as file:
        json.dump(report, file, indent=1)
    print("wrote", os.path.join(arguments.out, "report.json"))


if __name__ == "__main__":
    main()
