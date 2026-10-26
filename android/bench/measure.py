#!/usr/bin/env python3
"""Measures the benchmark apps (bench-compose: Kotlin Compose; bench-rs: compose-rs) on a
connected device, interleaving the two apps run by run, and writes a JSON report.

    measure.py --trace-processor PATH --out DIR [--runs N] [--compile MODE] [--allow-stale]
               [--scenarios NAME,NAME,...] [--locked-clocks STATE]
               [--optimizations parity|recommended|all|each|NAME,... | --subject [NAME=]LIST ...]
               [--traced-runs N] [--battery-limit CELSIUS] [--thermal-wait-limit SECONDS]
               [--navigations N]

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
  navigation   only when named (docs/17): the navigation screens' launcher, a warm-up navigation
               left out, then --navigations (20) navigations a run, each a tap on its button
               opening a heavy screen of new content, 1 s to settle, then back; --traced-runs
               traced runs (3), then --runs untraced ones, of each subject, interleaved. Traced:
               each navigation's latency from the tap's input event (`deliverInputEvent`'s
               `eventTimeNano`) to FrameTimeline's present of the frame whose doFrame holds the
               heavy screen's `navigation: heavy screen` section; that doFrame's main thread by
               phase (composition, layout, draw, …), its DrawFrame, the GPU's completion, its
               jank. Every run: the heavy frame's framestats (`dumpsys gfxinfo framestats`)

--scenarios picks some of them (all but navigation by default), comma-separated; the report
holds those run.
Each traced run also lists its slowest frames on each thread with their longest slices
(`slowest_frames`).

--optimizations (compose-rs's own, beyond what Compose does: `Optimization::ALL` in
crates/compose-ui/src/optimizations.rs, docs/07; passed to bench-rs as `-e optimizations` in
every mode, as without it bench-rs runs the library's default, the recommended set, D610):
  parity  (default) none of them, the hint session included (`none`): compose-rs does the work
          Compose does (docs/15, "Parity mode"; D568)
  recommended  the flags with a measured gain (Rust's `Optimization::RECOMMENDED`, D601)
  all     every one
  rs      no Compose: bench-rs alone at parity, for an A/B of two compose-rs builds installed
          in turn (D598; --allow-stale for the one not built in this checkout)
  each    no Compose: bench-rs at parity, then with each optimization alone, interleaved run
          by run (subjects `rs` and `rs+<name>`): each one's effect, for docs/14's backport table;
          --each-only NAME,... measures only those
  NAMES   a comma-separated list of names (`-name` takes one out: `all,-performance_hint`,
          `recommended,-performance_hint`)
--subject [NAME=]LIST, repeated, measures several bench-rs subjects beside Compose in one
invocation, interleaved run by run (mode `subjects`): each one bench-rs with `-e optimizations
LIST`, named NAME (by default `rs-LIST`; a LIST of more than one word needs a NAME), after Compose
(`--subject recommended --subject all`: subjects `compose`, `rs-recommended`, `rs-all`). It
replaces --optimizations, which then stays at its default.
The report records the mode and each subject's list (`"optimizations"`). --traced-runs sets the
traced runs of each subject in the scroll and animation scenarios (3).

Thermal guards: before each run (each startup round, each traced run and traced first frame), our
apps are stopped (the last run's would go on animating) and the device's thermal status
(`dumpsys thermalservice`: "Thermal Status"), battery temperature (`dumpsys battery`) and skin
temperature (the highest `TYPE_SKIN` sensor, Pixel's VIRTUAL-SKIN, which it throttles by) are
read; while the status is above 0, the battery above --battery-limit (36 °C) or, with
--skin-limit, the skin above it, the run waits, reading them every 10 s, at most
--thermal-wait-limit seconds (600; 0: until cool), then goes on and notes it (`"gave_up"`). A
sensor that can't be read is said, once (and a limit on it can't hold). The stress animation
heats the device fast: its `each` runs want --battery-limit 34 --skin-limit 35
--thermal-wait-limit 0, and --rotate to vary which subject runs last (D574). Each traced run
records the state before and after it (`"thermal"`), the startup rounds and first frames in the
report's `"thermal"`; the report records each subject's bits (`"optimizations"`, `"bits"`).

Locked clocks (D640): with --locked-clocks STATE (the state file of `lock_clocks.sh lock STATE`,
which locks each CPU policy's and the GPU's clock on a rooted device), each run and startup
round reads the clocks back before and after it, each traced first frame before it (each lock's
limits, and each CPU's current clock: a thermal cap shows there), and each trace's frequency
events must be the locked ones; if one moved, the run stops and the report keeps what ran before (`"moved"`). The
report records the locks and each run's readings (`"clocks_before"`, `"clocks_after"`), and the
DSU's, MIF's and BCI's time at each frequency during it (`"devfreq_residency"`): the power HAL
moves those at every launch and load reset, so they aren't locked. At locked clocks the traces
hold no frequency event, so "work" scales each core's running time by its locked frequency.
Every traced run reports each thread's running time on the little, mid and big cores
(`"cluster_ms"`), which locked clocks don't equalize: the scheduler and the hint sessions'
uclamp still place threads. With --fixed-placement as well, the state also locks the top-app
group's minimum utilization clamp (`lock_clocks.sh … placement=512`: /proc/vendor_sched/groups/
ta/uclamp_min on a Pixel, D641), which keeps the foreground app's threads off the little cores
whatever their load or boost; it is read back and checked as the clocks, and the report records
it (`"fixed_placement"`).

The traces include atrace's `power` category: Android 17's power HAL traces its ADPF counters
(the RenderThread's boost) there.

Before each run, our other apps (`rs.compose.*`) are force-stopped (D343).

Counters (D660): with --counters, an invocation runs counted runs instead of the timing and
traced ones (no trace processor needed): --counter-runs (15) runs of each subject in each
countable scenario (scroll, animation, stress_scroll, stress_animation, navigation; not the cold
starts: simpleperf attaches to a running process), interleaved, with the same compilation,
thermal guards and locked-clock checks. Each run counts the app's threads with the device's
simpleperf (`simpleperf stat -p PID --per-thread`, --counter-events: instructions:u,
instructions:k, cpu-cycles) while a workload on the device resets gfxinfo, waits for the
scenario's input (or 10 s), then reads gfxinfo: instructions and cycles per thread group (the main
thread, RenderThread, hwuiTask, the GPU driver's threads, binder, JIT, GC, ART's daemons, other)
and for the process, per frame drawn in the window, IPC, the kernel's share; the share of each
thread's running time its counters ran (1 unless multiplexed); and over the runs the median, the
extremes, the spread and the coefficient of variation.
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


def start(package, screen, wait=True, optimizations=None):
    flags = "-W " if wait else ""
    return shell(start_command(package, screen, flags, optimizations))


def start_command(package, screen, flags="", optimizations=None):
    """`am start` for `package`'s `screen`, with compose-rs's `optimizations` (bench-rs's
    `-e optimizations`; None: the app's default)."""
    extra = f" -e optimizations {optimizations}" if optimizations is not None else ""
    return f"am start {flags}-n {package}/.MainActivity -e screen {screen}{extra}"


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


def cold_start(package, screen, optimizations=None):
    fresh(package)
    time.sleep(1.0)
    output = start(package, screen, optimizations=optimizations)
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
    return parse_gfxinfo(shell(f"dumpsys gfxinfo {package}" + (" reset" if reset else "")))


def parse_gfxinfo(output):
    """`dumpsys gfxinfo`'s frame count, janky frames and percentiles."""
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
    atrace_categories: "power"
    atrace_apps: "{app}"
}} }} }}
duration_ms: {duration}
"""


# Android 17's power HAL traces its ADPF counters (each hint session's boost: `adpf.<pid>-…-min`,
# `boost_stats`) under atrace's `power` category only; Android 16's traced them with `gfx`.

# The trace buffer: enough for the feed's scenarios. The stress screens' traces hold several
# times more (HWUI traces each of its operations: 140 cells' layers each frame), and a ring
# buffer that wraps loses the process table scanned at the start, and with it every frame.
# Android 17 writes more (a feed scroll's trace about 64 MB, where Android 16's 64 MB buffer had
# room): three times as much now, and a run whose buffer wrapped stops (`check_buffer`).
TRACE_BUFFER_KB = 196608
STRESS_TRACE_BUFFER_KB = 786432


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


class TraceWrapped(SystemExit):
    """A trace whose ring buffer wrapped: its start is lost (the process table, the warm-up's
    input), so its frames can't be trusted."""


def buffer_overwritten(rows):
    """The bytes a trace's ring buffers overwrote (its `stats`' rows)."""
    return sum(int(float(row.get("value", "0") or 0)) for row in rows
               if row.get("name") == "traced_buf_bytes_overwritten")


def check_buffer(trace_processor, trace):
    """Raises TraceWrapped if the trace's buffer wrapped."""
    overwritten = buffer_overwritten(query(trace_processor, trace, """
        SELECT name, value FROM stats WHERE name = 'traced_buf_bytes_overwritten'"""))
    if overwritten:
        raise TraceWrapped(f"{trace}: the trace buffer wrapped ({overwritten} bytes overwritten): "
                           "a bigger buffer is needed")


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
    but on cadence) isn't counted as jank, as before, nor Android 17's "Non Animating" (a frame
    of an app not animating, which FrameTimeline tags "Non-perceivable Jank")."""
    kinds = ({kind.strip() for kind in jank_type.split(",")}
             - {"None", "Buffer Stuffing", "Non Animating", ""})
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
    `since` (a trace timestamp: after a warm-up) on. Stops if the trace's buffer wrapped."""
    check_buffer(trace_processor, trace)
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
    running = thread_running(trace_processor, trace, package, since)
    trace_clocks = check_trace_clocks(trace_processor, trace, f"{package}'s trace {trace}")
    if LOCKED["expected"] is not None:
        work = fixed_work_ms(running, capacities,
                             fixed_frequencies(LOCKED["expected"], LOCKED["cpus"]))
    else:
        work = work_ms(trace_processor, trace, package, capacities, since)
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
        "work_ms": work,
        # Each thread's running time on the little, mid and big cores: where it ran, which
        # locked clocks don't equalize (the scheduler and uclamp place threads).
        "cluster_ms": cluster_ms(running, capacities),
        "locked_clocks": trace_clocks,
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


# Rust's table of compose-rs's optimizations: their names, in bit order (D520).
OPTIMIZATIONS_SOURCE = os.path.join(os.path.dirname(ANDROID_DIR), "crates", "compose-ui", "src",
                                    "optimizations.rs")


def optimization_table(source):
    """Rust's table (crates/compose-ui/src/optimizations.rs): each `Optimization`'s name, bit
    and default, read from its own `bit:`, `name:` and `default:` fields, in bit order."""
    rows = re.findall(r'Optimization \{\s*bit: (\d+),\s*name: "([a-z_]+)",\s*default: (true|false),',
                      source)
    return sorted(((name, int(bit), default == "true") for bit, name, default in rows),
                  key=lambda row: row[1])


def optimization_names(source):
    """The optimizations' names in Rust's table, in bit order."""
    return [name for name, _, _ in optimization_table(source)]


def optimization_defaults(source):
    """The names of the optimizations on by default in Rust's table."""
    return [name for name, _, default in optimization_table(source) if default]


def optimization_recommended(source):
    """The names in Rust's `Optimization::RECOMMENDED` (crates/compose-ui/src/optimizations.rs),
    in its order: `Optimization::SINGLE_DRAWING_FADE` is `single_drawing_fade`."""
    found = re.search(r"pub const RECOMMENDED: &'static \[Optimization\] = &\[(.*?)\];", source,
                      re.DOTALL)
    if not found:
        raise SystemExit("no Optimization::RECOMMENDED in Rust's table")
    return [name.lower() for name in re.findall(r"Optimization::([A-Z_]+)", found.group(1))]


def optimization_bits(given, table, recommended=()):
    """The bits bench-rs's `-e optimizations` list sets, as Rust's `Optimizations::parse`
    (`none`, `all`, `recommended`, `default`, names, `-name`; None or empty: the defaults), each
    name's bit its `bit:` in `table` (`optimization_table`), `recommended` the names in Rust's
    `RECOMMENDED` (`optimization_recommended`)."""
    bit = {name: 1 << number for name, number, _ in table}
    every = sum(bit.values())
    defaults = sum(bit[name] for name, _, default in table if default)
    chosen = sum(bit[name] for name in recommended)
    if given is None or not given.strip():
        return defaults
    bits = 0
    for word in [word.strip() for word in given.split(",") if word.strip()]:
        on, name = (False, word[1:]) if word.startswith("-") else (True, word)
        if (name, on) == ("none", True):
            bits = 0
        elif (name, on) == ("all", True):
            bits = every
        elif (name, on) == ("recommended", True):
            bits = chosen
        elif (name, on) == ("default", True):
            bits = defaults
        else:
            bits = bits | bit[name] if on else bits & ~bit[name]
    return bits


# The names of sets in a list, beside the optimizations' own (`Optimizations::parse`).
SET_NAMES = ("none", "all", "recommended", "default")


def check_list(text, names):
    """The words of a bench-rs list, checked against `names` and the sets' names; stops the run
    on an unknown one."""
    words = [word.strip() for word in text.split(",") if word.strip()]
    unknown = [word for word in words
               if word.lstrip("-") not in names and word not in SET_NAMES]
    if unknown:
        raise SystemExit(f"unknown optimizations {unknown}; known: {', '.join(names)}")
    return words


def parse_subjects(given, names):
    """`--subject [NAME=]LIST`'s subjects: Compose, then a bench-rs subject for each, with `-e
    optimizations LIST`, named NAME or `rs-LIST` (a LIST of one word). Stops the run on an unknown
    name, a LIST of several words without a NAME, or two subjects of one name."""
    subjects = [("compose", APPS["compose"], None)]
    for text in given:
        name, _, listed = text.rpartition("=")
        words = check_list(listed, names)
        if not words:
            raise SystemExit(f"--subject {text!r}: an empty list")
        if not name:
            if len(words) > 1:
                raise SystemExit(f"--subject {text!r}: name a list of several words (NAME=LIST)")
            name = f"rs-{words[0]}"
        if not re.fullmatch(r"[A-Za-z0-9_+.-]+", name):
            raise SystemExit(f"--subject {text!r}: a name of letters, digits and _+.- only")
        if name in [subject for subject, _, _ in subjects]:
            raise SystemExit(f"--subject {text!r}: two subjects named {name}")
        subjects.append((name, APPS["rs"], ",".join(words)))
    return "subjects", subjects


def parse_optimizations(text, names, each_only=None):
    """The mode `--optimizations` names and the subjects it measures: (name, package,
    bench-rs's `-e optimizations`, None for the Compose app). `parity` (the default) runs
    bench-rs with none (`none`, the hint session off: Compose has none); `recommended` with the
    flags of Rust's `RECOMMENDED`; `all` with every one;
    `rs` bench-rs alone at parity (an A/B of two builds);
    `each` bench-rs alone, at parity and with each of `names` alone (or of `each_only`'s, a
    comma-separated list); else a list of names (each may be `-name`), checked against `names`.
    Stops the run on an unknown name."""
    text = (text or "parity").strip()
    compose = ("compose", APPS["compose"], None)
    if text == "parity":
        return "parity", [compose, ("rs", APPS["rs"], "none")]
    if text in ("all", "recommended"):
        return text, [compose, ("rs", APPS["rs"], text)]
    if text == "rs":
        return "rs", [("rs", APPS["rs"], "none")]
    if text == "each":
        chosen = names
        if each_only:
            chosen = [name.strip() for name in each_only.split(",") if name.strip()]
            unknown = [name for name in chosen if name not in names]
            if unknown:
                raise SystemExit(f"unknown optimizations {unknown}; known: {', '.join(names)}")
        return "each", [("rs", APPS["rs"], "none")] + [
            (f"rs+{name}", APPS["rs"], name) for name in chosen]
    words = check_list(text, names)
    return "list", [compose, ("rs", APPS["rs"], ",".join(words))]


# Thermal guards (see above): how often and how long a run waits for the device to cool.
THERMAL_POLL_S = 10
THERMAL = {"battery_limit_c": 36.0, "wait_limit_s": 600, "skin_limit_c": None}


def thermal_status(output):
    """The current thermal status from `dumpsys thermalservice` (0: none), or None."""
    match = re.search(r"^Thermal Status: (\d+)", output, re.M)
    return int(match.group(1)) if match else None


def battery_celsius(output):
    """The battery's temperature from `dumpsys battery` (tenths of a degree), or None."""
    match = re.search(r"^\s*temperature: (-?\d+)", output, re.M)
    return int(match.group(1)) / 10 if match else None


def skin_celsius(output):
    """The skin temperature from `dumpsys thermalservice`: the highest of its skin sensors
    (`mType=3`, `TYPE_SKIN`: Pixel's VIRTUAL-SKIN, another device's own), the HAL's current
    values if listed, else the cached ones (which can be stale), or None without one."""
    current = output.split("Current temperatures from HAL:", 1)
    for text in ([current[1]] if len(current) > 1 else []) + [output]:
        values = [float(value) for value in
                  re.findall(r"Temperature\{mValue=([-\d.]+), mType=3,", text)]
        if values:
            return round(max(values), 1)
    return None


# The sensors a run found missing, said once each.
MISSING_SENSORS = set()


def thermal_state(dumpsys=None):
    """The thermal status, battery and skin temperatures (`dumpsys`: a function of a dumpsys
    service's name to its output, the device's by default). A sensor it can't read is None,
    said once; a limit set on one that can't be read is said too, as it can't hold."""
    dumpsys = dumpsys or (lambda service: shell(f"dumpsys {service}"))
    thermal = dumpsys("thermalservice")
    state = {"status": thermal_status(thermal), "battery_c": battery_celsius(dumpsys("battery")),
             "skin_c": skin_celsius(thermal)}
    for sensor, limit in (("status", None), ("battery_c", THERMAL["battery_limit_c"]),
                          ("skin_c", THERMAL["skin_limit_c"])):
        if state[sensor] is None and sensor not in MISSING_SENSORS:
            MISSING_SENSORS.add(sensor)
            print(f"thermal: no {sensor} read from the device"
                  + (f"; its limit ({limit}) can't hold" if limit is not None else ""))
    return state


def too_warm(state, battery_limit_c, skin_limit_c=None):
    """The thermal status above 0, the battery above `battery_limit_c`, or (with a limit) the
    skin above `skin_limit_c`: Pixel throttles by its skin temperature first."""
    return ((state.get("status") or 0) > 0
            or (state.get("battery_c") is not None and state["battery_c"] > battery_limit_c)
            or (skin_limit_c is not None and state.get("skin_c") is not None
                and state["skin_c"] > skin_limit_c))


def wait_until_cool(what, read=thermal_state, sleep=time.sleep, battery_limit_c=None,
                    limit_s=None, skin_limit_c=None):
    """Waits while the device is too warm (thermal status above 0, battery above the limit),
    reading it every THERMAL_POLL_S, at most `limit_s` (0: until it's cool, however long).
    Returns the state the run starts in
    (`"before"`), the seconds waited and whether it gave up (still too warm)."""
    battery_limit_c = THERMAL["battery_limit_c"] if battery_limit_c is None else battery_limit_c
    limit_s = THERMAL["wait_limit_s"] if limit_s is None else limit_s
    skin_limit_c = THERMAL["skin_limit_c"] if skin_limit_c is None else skin_limit_c
    state = read()
    waited = 0
    while too_warm(state, battery_limit_c, skin_limit_c) and (limit_s == 0 or waited < limit_s):
        if waited == 0:
            print(f"{what}: waiting for the device to cool ({state})")
        sleep(THERMAL_POLL_S)
        waited += THERMAL_POLL_S
        state = read()
    gave_up = too_warm(state, battery_limit_c, skin_limit_c)
    if gave_up:
        print(f"{what}: still too warm after {waited} s ({state}); measuring anyway")
    return {"before": state, "waited_s": waited, "gave_up": gave_up}


def cool_down(what):
    """Our apps stopped (the last run's would go on animating, heating the device while it
    waits), then `wait_until_cool`; with --locked-clocks, the clocks checked (`check_clocks`)
    and the devfreqs' residency noted, for `finish_thermal`."""
    for package in APPS.values():
        stop(package)
    thermal = wait_until_cool(what)
    clocks = check_clocks(what)
    if clocks is not None:
        thermal["what"] = what
        thermal["clocks_before"] = {name: value for name, value in clocks.items()
                                    if name != "residency"}
        thermal["residency_before"] = clocks["residency"]
    return thermal


# ---- Locked clocks (lock_clocks.sh, D640) ---------------------------------------------------------

# With --locked-clocks: the frequency (kHz) each locked target holds (`policy0`, …, `gpu`), and
# each CPU policy's CPUs, read at the start.
LOCKED = {"expected": None, "cpus": {}}


class ClocksMoved(Exception):
    """A locked clock read back other than its lock: the run measured something else."""


def locked_clocks(state):
    """The frequencies lock_clocks.sh locked (its STATE file's `locked:<name> <kHz>` lines)."""
    locked = {}
    for line in state.splitlines():
        if line.startswith("locked:"):
            name, khz = line[len("locked:"):].split()
            locked[name] = int(khz)
    if not locked:
        raise SystemExit("--locked-clocks: the state file holds no locked clock "
                         "(lock_clocks.sh lock STATE first)")
    return locked


def load_locked_clocks(path):
    """`locked_clocks` of the file at `path`, or None without one."""
    if path is None:
        return None
    with open(path) as file:
        return locked_clocks(file.read())


# Each CPU policy's limits, current clock and CPUs; the GPU's (Mali); the DSU's, MIF's and BCI's
# time in each frequency (ms), which the power HAL moves itself and the lock leaves alone.
CLOCKS_COMMAND = (
    "for p in /sys/devices/system/cpu/cpufreq/policy*; do echo ${p##*/} "
    "$(cat $p/scaling_min_freq) $(cat $p/scaling_max_freq) $(cat $p/scaling_cur_freq) "
    "$(cat $p/related_cpus); done; "
    "for d in /sys/devices/platform/*.mali; do [ -e $d/scaling_max_freq ] && echo gpu "
    "$(cat $d/scaling_min_freq) $(cat $d/scaling_max_freq) $(cat $d/cur_freq); done; "
    "for d in /sys/class/devfreq/*; do n=${d##*devfreq_}; case $n in dsu|mif|bci) "
    "echo \"## $n\"; cat $d/time_in_state;; esac; done; "
    # The top-app group's minimum utilization clamp (lock_clocks.sh's `placement`, D641).
    "[ -e /proc/vendor_sched/groups/ta/uclamp_min ] && "
    "echo placement $(cat /proc/vendor_sched/groups/ta/uclamp_min); true")


def parse_clocks(output):
    """`CLOCKS_COMMAND`'s output: each policy's and the GPU's {"min", "max", "cur"} (kHz; a
    policy's "cpus" too), and "residency": each devfreq's ms by kHz."""
    clocks = {"residency": {}}
    devfreq = None
    for line in output.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0] == "placement" and parts[1].isdigit():
            devfreq = None
            clocks["placement"] = {"value": int(parts[1])}
        elif line.startswith("## ") and len(parts) == 2:
            devfreq = parts[1]
            clocks["residency"][devfreq] = {}
        elif devfreq and len(parts) == 2 and all(part.isdigit() for part in parts):
            clocks["residency"][devfreq][int(parts[0])] = int(parts[1])
        elif len(parts) >= 4 and (parts[0].startswith("policy") or parts[0] == "gpu") \
                and all(part.isdigit() for part in parts[1:]):
            devfreq = None
            clocks[parts[0]] = {"min": int(parts[1]), "max": int(parts[2]), "cur": int(parts[3])}
            if parts[0].startswith("policy"):
                clocks[parts[0]]["cpus"] = [int(cpu) for cpu in parts[4:]]
    return clocks


def read_clocks():
    return parse_clocks(shell(CLOCKS_COMMAND))


def clocks_moved(expected, clocks):
    """What of `expected` (`locked_clocks`) `clocks` (`parse_clocks`) doesn't hold: a limit
    other than its lock, or a CPU running at another clock (a thermal cap). The GPU's current
    clock isn't checked: it reads its lowest step when powered off."""
    moved = []
    for name, khz in expected.items():
        clock = clocks.get(name)
        if clock is None:
            moved.append(f"{name}: not read")
        elif name == "placement":
            if clock["value"] != khz:
                moved.append(f"placement: top-app uclamp.min {clock['value']} (locked at {khz})")
        elif clock["min"] != khz or clock["max"] != khz or (
                name.startswith("policy") and clock["cur"] != khz):
            moved.append(f"{name}: min {clock['min']}, max {clock['max']}, now {clock['cur']} "
                         f"(locked at {khz})")
    return moved


def check_placement(expected, fixed_placement):
    """The top-app group's locked uclamp.min with --fixed-placement, None without; stops unless
    the flag and lock_clocks.sh's state agree (a lock of `placement` changes where threads run,
    which a run must say)."""
    locked = (expected or {}).get("placement")
    if fixed_placement and locked is None:
        raise SystemExit("--fixed-placement: --locked-clocks's state locks no placement "
                         "(lock_clocks.sh lock STATE … placement=UCLAMP)")
    if not fixed_placement and locked is not None:
        raise SystemExit("--locked-clocks: the state locks the placement (top-app uclamp.min "
                         f"{locked}): pass --fixed-placement")
    return locked


def check_clocks(what):
    """With --locked-clocks, the clocks read back (`read_clocks`), raising ClocksMoved if a lock
    doesn't hold; None without."""
    if LOCKED["expected"] is None:
        return None
    clocks = read_clocks()
    moved = clocks_moved(LOCKED["expected"], clocks)
    if moved:
        raise ClocksMoved(f"{what}: the locked clocks moved: {'; '.join(moved)}")
    return clocks


def residency_delta(before, after):
    """Each devfreq's time (ms) by kHz between two `parse_clocks` residencies, and its mean."""
    out = {}
    for name, times in after.items():
        spent = {khz: ms - before.get(name, {}).get(khz, 0) for khz, ms in times.items()}
        spent = {khz: ms for khz, ms in spent.items() if ms > 0}
        total = sum(spent.values())
        out[name] = {"ms": spent,
                     "mean_mhz": round(sum(khz * ms for khz, ms in spent.items()) / total / 1000, 1)
                     if total else None}
    return out


def trace_clocks_moved(expected, cpus, cpu_rows, gpu_rows):
    """What a trace's frequency events say moved: `cpu_rows` (cpu, min, max kHz) of its
    `cpufreq` counters, `gpu_rows` (gpu id, min, max) of its `gpufreq` ones (0: powered off;
    the highest id is the clock lock_clocks.sh sets: Mali's shaders)."""
    locked_cpu = {cpu: expected[name] for name, listed in cpus.items() if name in expected
                  for cpu in listed}
    moved = []
    for cpu, low, high in cpu_rows:
        if cpu in locked_cpu and (low != locked_cpu[cpu] or high != locked_cpu[cpu]):
            moved.append(f"cpu {cpu} at {low} to {high} in the trace "
                         f"(locked at {locked_cpu[cpu]})")
    if "gpu" in expected and gpu_rows:
        gpu, low, high = max(gpu_rows)
        if {low, high} - {0, expected["gpu"]}:
            moved.append(f"the GPU at {low} to {high} in the trace (locked at {expected['gpu']})")
    return moved


def fixed_frequencies(expected, cpus):
    """Each locked CPU's frequency (kHz), by CPU."""
    return {cpu: expected[name] for name, listed in cpus.items() if name in expected
            for cpu in listed}


def fixed_work_ms(running, capacities, fixed):
    """`work_ms`'s metric at locked clocks, whose traces hold no frequency event (the clocks
    don't change): `running` (thread: {cpu: ns}) scaled by each core's locked frequency and
    capacity. None if a core ran without a locked frequency."""
    work = {}
    for thread, by_cpu in running.items():
        total = 0.0
        for cpu, ns in by_cpu.items():
            if cpu not in fixed or cpu not in capacities:
                return None
            capacity, max_freq = capacities[cpu]
            total += ns * fixed[cpu] / max_freq * capacity / 1024
        work[thread] = total / 1e6
    return work


def cluster_ms(running, capacities):
    """Each thread's running time (ms) on the little, mid and big cores (`core_kind`)."""
    out = {}
    for thread, by_cpu in running.items():
        kinds = {"little": 0.0, "mid": 0.0, "big": 0.0}
        for cpu, ns in by_cpu.items():
            kind = core_kind(cpu, capacities)
            kinds[kind] = kinds.get(kind, 0.0) + ns / 1e6
        out[thread] = {kind: round(ms, 3) for kind, ms in kinds.items()}
    return out


def thread_running(trace_processor, trace, package, since=0):
    """The main thread's and RenderThread's running time (ns) by CPU, from `since` on."""
    rows = query(trace_processor, trace, f"""
        SELECT CASE WHEN t.name = 'RenderThread' THEN 'render' ELSE 'main' END AS thread,
               s.cpu AS cpu, SUM(s.dur) AS running
        FROM sched s JOIN thread t USING (utid) JOIN process p USING (upid)
        WHERE p.name = '{package}' AND s.dur > 0 AND s.ts >= {since}
          AND (t.tid = p.pid OR t.name = 'RenderThread')
        GROUP BY thread, s.cpu
        """)
    running = {"main": {}, "render": {}}
    for row in rows:
        if row.get("cpu", "").isdigit() and row.get("running", "").isdigit():
            running[row["thread"]][int(row["cpu"])] = int(row["running"])
    return running


def check_trace_clocks(trace_processor, trace, what):
    """With --locked-clocks, raises ClocksMoved if the trace's frequency events left the locks;
    returns what they held (None without)."""
    if LOCKED["expected"] is None:
        return None
    cpu_rows = [(int(r["cpu"]), int(float(r["low"])), int(float(r["high"]))) for r in query(
        trace_processor, trace, """
        SELECT t.cpu AS cpu, MIN(c.value) AS low, MAX(c.value) AS high FROM counter c
        JOIN cpu_counter_track t ON c.track_id = t.id WHERE t.name = 'cpufreq' GROUP BY t.cpu
        """) if r.get("cpu", "").isdigit()]
    gpu_rows = [(int(r["gpu"]), int(float(r["low"])), int(float(r["high"]))) for r in query(
        trace_processor, trace, """
        SELECT t.gpu_id AS gpu, MIN(c.value) AS low, MAX(c.value) AS high FROM counter c
        JOIN gpu_counter_track t ON c.track_id = t.id WHERE t.name = 'gpufreq'
        GROUP BY t.gpu_id
        """) if r.get("gpu", "").isdigit()]
    moved = trace_clocks_moved(LOCKED["expected"], LOCKED["cpus"], cpu_rows, gpu_rows)
    if moved:
        raise ClocksMoved(f"{what}: {'; '.join(moved)}")
    return {"cpu": cpu_rows, "gpu": gpu_rows}


def finish_thermal(thermal):
    """A run's thermal record completed after it: the state after, and with --locked-clocks the
    clocks read back (raising ClocksMoved if they moved) and the devfreqs' residency during it."""
    thermal["after"] = thermal_state()
    clocks = check_clocks(thermal.get("what", "a run"))
    if clocks is not None:
        thermal["clocks_after"] = {name: value for name, value in clocks.items()
                                   if name != "residency"}
        before = thermal.pop("residency_before", {})
        thermal["devfreq_residency"] = residency_delta(before, clocks["residency"])
    return thermal


def target_sdk(output):
    """An app's targetSdk from `dumpsys package <name>`, or None."""
    match = re.search(r"\btargetSdk=(\d+)", output)
    return int(match.group(1)) if match else None


# The scenarios, in the order they run (docs/12, docs/15).
SCENARIOS = ("startup", "scroll", "animation", "stress_startup", "stress_scroll",
             "stress_animation")
# Scenarios run only when named, after those (docs/17), so `all` stays the rounds' set.
NAMED_SCENARIOS = ("navigation",)


def parse_scenarios(text):
    """The scenarios `--scenarios` names (comma-separated; empty or "all": SCENARIOS), in
    SCENARIOS' order, then NAMED_SCENARIOS'. Stops the run on an unknown name."""
    names = [name.strip() for name in (text or "").split(",") if name.strip()]
    if not names or names == ["all"]:
        return list(SCENARIOS)
    known = SCENARIOS + NAMED_SCENARIOS
    unknown = sorted(set(names) - set(known))
    if unknown:
        raise SystemExit(f"unknown scenarios {unknown}; known: {', '.join(known)}")
    return [name for name in known if name in names]


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
               prefix="scroll", buffer_kb=TRACE_BUFFER_KB, optimizations=None):
    """One traced run of 10 flings through the feed (or `screen`), after a warm-up swipe: the
    display, idle since the app started, resyncs its vsync on the first input, which
    FrameTimeline counts as jank no app causes. The warm-up's frames are left out of the stats
    (from the first input after it), and gfxinfo is reset after it. Returns the memory after
    start, and the stats."""
    thermal = cool_down(f"{prefix} {name} {run}")
    fresh(package)
    time.sleep(1.0)
    start(package, screen, optimizations=optimizations)
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
    stats["thermal"] = finish_thermal(thermal)
    return after_start, stats


def startup_scenario(tp, out_dir, runs, screens, subjects, thermal_log):
    """Cold starts of each of `screens`, interleaved by subject, `runs` times; then one traced
    cold start of each for the first frame's breakdown. Each round waits for a cool device
    first, noted in `thermal_log`."""
    startup = {name: {screen: [] for screen in screens} for name, _, _ in subjects}
    for run in range(runs):
        thermal = cool_down(f"startup {'/'.join(screens)} round {run}")
        for screen in screens:
            for name, package, optimizations in subjects:
                startup[name][screen].append(cold_start(package, screen, optimizations))
        thermal_log.append({"screens": list(screens), "round": run, **finish_thermal(thermal)})
    startup_ms = {
        name: {screen: {"runs": times, "median": statistics.median(times), "min": min(times),
                        "max": max(times)} for screen, times in by_screen.items()}
        for name, by_screen in startup.items()
    }
    print("startup", json.dumps(startup_ms, indent=1))
    first = {}
    for name, package, optimizations in subjects:
        first[name] = {}
        for screen in screens:
            thermal = cool_down(f"first frame {name} {screen}")
            thermal_log.append({"first_frame": name, "screen": screen, **thermal})
            fresh(package)
            time.sleep(1.0)
            process, path = start_trace(package, 5000, f"start-{name}-{screen}")
            start(package, screen, wait=False, optimizations=optimizations)
            trace = finish_trace(process, path, out_dir)
            first[name][screen] = first_frame(tp, trace, package)
    print("first frame", json.dumps(first, indent=1))
    return startup_ms, first


def scroll_scenario(tp, out_dir, capacities, screen, prefix, subjects, runs=3,
                    buffer_kb=TRACE_BUFFER_KB):
    """`runs` traced scroll runs of each subject through `screen`, interleaved, with the memory
    after start and after the flings."""
    memory = {name: {"after_start": [], "after_scroll": []} for name, _, _ in subjects}
    scroll = {name: [] for name, _, _ in subjects}
    for run in range(runs):
        for name, package, optimizations in subjects:
            after_start, stats = traced(
                lambda: scroll_run(tp, out_dir, package, name, run, capacities, screen, prefix,
                                   buffer_kb, optimizations),
                f"{prefix} {name} {run}")
            memory[name]["after_start"].append(after_start)
            scroll[name].append(stats)
            memory[name]["after_scroll"].append(meminfo(package))
            print(prefix, name, run, json.dumps(stats))
    return memory, scroll


def animation_scenario(tp, out_dir, capacities, screen, prefix, subjects, runs=3, memory=False,
                       buffer_kb=TRACE_BUFFER_KB):
    """`runs` traced runs of each subject showing `screen` for 10 s untouched, interleaved, 3 s
    after it started. With `memory`, each run also has the memory before the trace (`after_start`;
    `dumpsys meminfo` collects garbage, so it's taken before gfxinfo's reset) and after it
    (`after_animation`), and HWUI's GPU caches at both points (`gpu_after_start`,
    `gpu_after_animation`)."""
    animation = {name: [] for name, _, _ in subjects}
    for run in range(runs):
        for name, package, optimizations in subjects:
            def animation_run():
                thermal = cool_down(f"{prefix} {name} {run}")
                fresh(package)
                time.sleep(1.0)
                start(package, screen, optimizations=optimizations)
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
                stats["thermal"] = finish_thermal(thermal)
                return stats
            stats = traced(animation_run, f"{prefix} {name} {run}")
            animation[name].append(stats)
            print(prefix, name, run, json.dumps(stats))
    return animation


# ---- The navigation scenario (docs/17) ---------------------------------------------------------

# Navigations per run, after a warm-up one left out (--navigations).
NAVIGATIONS = 20
# How long a navigation waits after the tap (the heavy screen's frames settle) and after back
# (the launcher's).
NAVIGATION_SETTLE_S = 1.0
NAVIGATION_BACK_S = 1.0
# What a navigation costs beside its waits, at most (adb's commands): the trace's length.
NAVIGATION_OVERHEAD_S = 1.0
# The empty section each app's heavy screen starts its composition with.
NAVIGATION_MARKER = "navigation: heavy screen"
# The heavy frames' HWUI operations are traced a slice each.
NAVIGATION_TRACE_BUFFER_KB = 393216
# A tap's input event is the last one delivered before the heavy frame, at most this long before.
NAVIGATION_INPUT_WINDOW_NS = 1_000_000_000

# Where a frame's main thread goes (`frame_phases`): a slice's own time (its duration less its
# children's) counts in the phase of the nearest slice, itself or an ancestor inside the frame,
# named here; else in "other". Compose 1.12.1's sections, compose-rs's, the view system's.
FRAME_PHASES = {
    "Compose:recompose": "composition", "Compose:applyChanges": "composition",
    "Compose:onRemembered": "composition", "Compose:unobserve": "composition",
    "Compose:onForgotten": "composition", "Compose:sideeffects": "composition",
    "Compose:Composer.dispose": "composition",
    "Recomposer:recompose": "composition", "compose-rs: recompose": "composition",
    "compose-rs: tasks": "composition",
    "AndroidOwner:measureAndLayout": "layout", "AndroidOwner:onMeasure": "layout",
    "AndroidOwner:onLayout": "layout", "Compose:onPositionedCallbacks": "layout",
    "measure": "layout", "layout": "layout",
    "compose-rs: layout": "layout", "compose-rs: recompose and layout again": "layout",
    "TextStringSimpleNode::measure": "text_layout", "TextAnnotatedStringNode:measure": "text_layout",
    "TextLayout:initLayout": "text_layout",
    "compose-rs: text layout": "text_layout",
    "AndroidOwner:draw": "draw", "compose-rs: draw": "draw",
    "compose-rs: serialize": "serialize", "compose-rs: take drawing": "serialize",
    "compose-rs: replay": "replay",
    "Record View#draw()": "record",
    "postAndWait": "sync",
    "input": "input",
}


def screen_center(wm_size_output):
    """The screen's center in pixels, from `wm size` (its override if any), or None."""
    sizes = dict(re.findall(r"(Physical|Override) size: (\d+x\d+)", wm_size_output))
    size = sizes.get("Override") or sizes.get("Physical")
    if not size:
        return None
    width, height = (int(value) for value in size.split("x"))
    return width // 2, height // 2


def framestats_frames(output):
    """The frames of `dumpsys gfxinfo <package> framestats`: each `---PROFILEDATA---` section's
    rows as {column: int}, by its header's names (`IntendedVsync`, `FrameCompleted`, …)."""
    frames = []
    for section in output.split("---PROFILEDATA---")[1::2]:
        lines = [line.strip() for line in section.strip().splitlines() if line.strip()]
        if not lines:
            continue
        header = [name for name in lines[0].split(",") if name]
        for line in lines[1:]:
            cells = [cell for cell in line.split(",") if cell != ""]
            if len(cells) < len(header) or not all(re.fullmatch(r"-?\d+", c) for c in cells):
                continue
            frames.append(dict(zip(header, (int(cell) for cell in cells))))
    return frames


def latest_intended_vsync(frames):
    """The latest `IntendedVsync` of `frames` (`framestats_frames`), 0 without one."""
    return max((frame.get("IntendedVsync", 0) for frame in frames), default=0)


def framestats_heavy_frame(frames, after=0):
    """The slowest frame (`IntendedVsync` to `FrameCompleted`) of `frames` (`framestats_frames`)
    intended after `after` (an `IntendedVsync`), frames with flags (a window change, a skipped
    frame) left out, and its phases in ms: from its vsync to the main thread's start (its
    Choreographer callbacks'), the callbacks (Compose's recomposition, compose-rs's whole frame),
    the traversal's measure and layout, its drawing, the RenderThread's sync and drawing up to the
    frame's completion; and from its vsync, the GPU's completion and the display's present (when
    the device reports them). None without one."""
    candidates = [frame for frame in frames if frame.get("IntendedVsync", 0) > after
                  and frame.get("Flags", 0) == 0 and frame.get("FrameCompleted", 0) > 0]
    if not candidates:
        return None
    frame = max(candidates, key=lambda f: f["FrameCompleted"] - f["IntendedVsync"])
    start = frame["IntendedVsync"]

    def span(begin, end):
        if frame.get(begin, 0) <= 0 or frame.get(end, 0) <= 0:
            return None
        return round((frame[end] - frame[begin]) / 1e6, 3)

    def since_vsync(name):
        value = frame.get(name, 0)
        return round((value - start) / 1e6, 3) if value > 0 else None

    return {"vsync_id": frame.get("FrameTimelineVsyncId"), "intended_vsync": start,
            "frame_ms": since_vsync("FrameCompleted"),
            "start_delay_ms": since_vsync("AnimationStart"),
            "animation_ms": span("AnimationStart", "PerformTraversalsStart"),
            "traversal_ms": span("PerformTraversalsStart", "DrawStart"),
            "draw_ms": span("DrawStart", "SyncQueued"),
            "sync_ms": span("SyncStart", "IssueDrawCommandsStart"),
            "render_ms": span("IssueDrawCommandsStart", "FrameCompleted"),
            "gpu_completed_ms": since_vsync("GpuCompleted"),
            "present_ms": since_vsync("DisplayPresentTime")}


def event_time_ns(slice_name):
    """The input event's own time (`eventTimeNano=`, CLOCK_MONOTONIC) in a `deliverInputEvent`
    slice's name (Android 12 and later), or None."""
    match = re.search(r"eventTimeNano=(\d+)", slice_name)
    return int(match.group(1)) if match else None


def monotonic_offset(snapshots):
    """The trace's clock (BOOTTIME) less CLOCK_MONOTONIC, from the trace's clock snapshots (rows
    of `clock_snapshot`: `snapshot_id`, `clock_name`, `clock_value`), or None."""
    by_snapshot = {}
    for row in snapshots:
        by_snapshot.setdefault(row.get("snapshot_id"), {})[row.get("clock_name")] = row.get(
            "clock_value")
    for clocks in by_snapshot.values():
        boottime, monotonic = clocks.get("BOOTTIME"), clocks.get("MONOTONIC")
        if boottime and monotonic and boottime.isdigit() and monotonic.isdigit():
            return int(boottime) - int(monotonic)
    return None


def frame_phases(frame_dur, slices):
    """A frame's main-thread time by phase (`FRAME_PHASES`), in ms. `slices` are the frame's
    descendants as (id, parent id, name, dur ns); the frame's own time is "other", so the phases
    sum to the frame's duration."""
    by_id = {slice_id: (parent, name, dur) for slice_id, parent, name, dur in slices}
    children = {}
    for _, parent, _, dur in slices:
        children[parent] = children.get(parent, 0) + dur

    def phase(slice_id):
        while slice_id in by_id:
            parent, name, _ = by_id[slice_id]
            if name in FRAME_PHASES:
                return FRAME_PHASES[name]
            slice_id = parent
        return "other"

    phases = {}
    for slice_id, (_, _, dur) in by_id.items():
        own = max(dur - children.get(slice_id, 0), 0)
        name = phase(slice_id)
        phases[name] = phases.get(name, 0.0) + own / 1e6
    direct = sum(dur for parent, _, dur in by_id.values() if parent not in by_id)
    phases["other"] = phases.get("other", 0.0) + max(frame_dur - direct, 0) / 1e6
    return {name: round(value, 3) for name, value in phases.items()}


def overlap_by(rows, start, end, key):
    """The time (ms) of `rows` ((ts, dur, …) in ns) inside [start, end), by `key(row)`."""
    totals = {}
    for row in rows:
        inside = min(row[0] + row[1], end) - max(row[0], start)
        if inside > 0:
            totals[key(row)] = totals.get(key(row), 0.0) + inside / 1e6
    return {name: round(value, 3) for name, value in totals.items()}


def core_kind(cpu, capacities):
    """A CPU's kind by its capacity among the device's (`cpu_capacities`): "little" (the lowest),
    "big" (the highest), "mid", or "unknown"."""
    if not capacities or cpu not in capacities:
        return "unknown"
    kinds = sorted({capacity for capacity, _ in capacities.values()})
    capacity = capacities[cpu][0]
    if capacity == kinds[0]:
        return "little"
    return "big" if capacity == kinds[-1] else "mid"


def window_work_ms(running, frequencies, capacities, start, end):
    """The work (`work_ms`'s metric: ms on the fastest core at its highest frequency) of
    `running` ((ts, dur, cpu) in ns) inside [start, end), each part scaled by its core's
    frequency then (`frequencies`: by CPU, sorted (ts, dur, kHz)) and capacity. None without
    capacities or frequencies."""
    if not capacities or not frequencies:
        return None
    work = 0.0
    for ts, dur, cpu in running:
        begin, finish = max(ts, start), min(ts + dur, end)
        if finish <= begin or cpu not in capacities:
            continue
        capacity, max_freq = capacities[cpu]
        for freq_ts, freq_dur, freq in frequencies.get(cpu, ()):
            inside = min(finish, freq_ts + freq_dur) - max(begin, freq_ts)
            if inside > 0:
                work += inside * freq / max_freq * capacity / 1024
    return round(work / 1e6, 3)


def navigation_records(markers, frames, inputs, render_frames, gpu_waits, timeline, expected,
                       offset=None):
    """Each navigation's first frame of the heavy screen, from a trace's rows (times in ns):
    `markers` the heavy screen's sections' starts; `frames` the main thread's doFrames as (id, ts,
    dur, vsync id); `inputs` its `deliverInputEvent`s as (ts, name); `render_frames` the
    RenderThread's `DrawFrames <vsync id>` as (ts, dur, vsync id or None); `gpu_waits` the GPU
    completion thread's waits as (ts, dur); `timeline` and `expected` FrameTimeline's actual and
    expected slices by vsync id, as (ts, dur, jank type, present type); `offset` the trace's clock
    less CLOCK_MONOTONIC (`monotonic_offset`), which places the input events' own times.

    A navigation's frame is the doFrame holding the marker; its input the last event delivered
    before that doFrame (the tap's up, whose click switches the screen), within
    NAVIGATION_INPUT_WINDOW_NS and after the previous navigation's frame; its present
    FrameTimeline's actual present (its actual slice's end). The times (`…_ms`) are from that
    event's delivery to the app's main thread. `input tap` stamps its down and up with the same
    time and injects the up once the app has handled the down, so the up's own time is the down's:
    `down_to_present_ms`, from it when the offset places it (`"event_time"` in `"input"`),
    includes the app's handling of the down."""
    records = []
    previous_end = 0
    frames = sorted(frames, key=lambda frame: frame[1])
    inputs = sorted(inputs)
    render_frames = sorted(render_frames, key=lambda frame: frame[0])
    gpu_waits = sorted(gpu_waits)
    for marker in sorted(markers):
        frame = next((f for f in frames if f[1] <= marker < f[1] + f[2]), None)
        if frame is None or frame[1] < previous_end:
            continue
        frame_id, frame_ts, frame_dur, vsync = frame
        delivered = [(ts, name) for ts, name in inputs if previous_end <= ts < frame_ts
                     and frame_ts - ts <= NAVIGATION_INPUT_WINDOW_NS]
        previous_end = frame_ts + frame_dur
        if not delivered:
            continue
        delivery_ts, name = delivered[-1]
        event = event_time_ns(name)
        input_ts, event_ts = delivery_ts, None
        if (offset is not None and event is not None
                and 0 <= delivery_ts - (event + offset) < NAVIGATION_INPUT_WINDOW_NS):
            event_ts = event + offset

        def after_input(ts):
            return None if ts is None else round((ts - input_ts) / 1e6, 3)

        render = next((r for r in render_frames if r[2] == vsync), None) or next(
            (r for r in render_frames if r[2] is None and r[0] >= frame_ts), None)
        # The GPU's wait for this frame starts after its DrawFrames and before the next one.
        following = next((r[0] for r in render_frames if render and r[0] > render[0]), None)
        gpu = next((g for g in gpu_waits if render and g[0] >= render[0]
                    and (following is None or g[0] < following)), None)
        actual, planned = timeline.get(vsync), expected.get(vsync)
        present = actual[0] + actual[1] if actual else None
        expected_present = planned[0] + planned[1] if planned else None
        records.append({
            "vsync_id": vsync, "frame_id": frame_id, "input_ts": input_ts,
            "input": "event_time" if event_ts is not None else "delivery",
            "down_to_present_ms": round((present - event_ts) / 1e6, 3)
            if present is not None and event_ts is not None else None,
            "down_to_delivery_ms": round((delivery_ts - event_ts) / 1e6, 3)
            if event_ts is not None else None,
            "frame_start_ms": after_input(frame_ts),
            "main_end_ms": after_input(frame_ts + frame_dur),
            "render_end_ms": after_input(render[0] + render[1]) if render else None,
            "gpu_end_ms": after_input(gpu[0] + gpu[1]) if gpu else None,
            "input_to_present_ms": after_input(present),
            "do_frame_ms": round(frame_dur / 1e6, 3),
            "render_ms": round(render[1] / 1e6, 3) if render else None,
            "gpu_wait_ms": round(gpu[1] / 1e6, 3) if gpu else None,
            "late_ms": round((present - expected_present) / 1e6, 3)
            if present is not None and expected_present is not None else None,
            "jank_type": actual[2] if actual else None,
            "present_type": actual[3] if actual else None,
        })
    return records


def navigation_trace(trace_processor, trace, package, capacities):
    """The navigations of a traced run (`navigation_records`), each with its main thread's
    phases (`frame_phases`), its main thread's states and cores while its doFrame ran, its
    RenderThread's cores while its DrawFrame ran, and the app's other janky frames from its input
    to 100 ms after its present."""
    def rows(sql):
        return query(trace_processor, trace, sql)

    check_buffer(trace_processor, trace)

    app = f"""FROM slice s JOIN thread_track tt ON s.track_id = tt.id
        JOIN thread t USING (utid) JOIN process p USING (upid) WHERE p.name = '{package}'"""
    markers = [int(r["ts"]) for r in rows(
        f"SELECT s.ts AS ts {app} AND t.tid = p.pid AND s.name = '{NAVIGATION_MARKER}'")]
    frames = [(int(r["id"]), int(r["ts"]), int(r["dur"]), int(r["name"].split()[1])) for r in rows(
        f"""SELECT s.id AS id, s.ts AS ts, s.dur AS dur, s.name AS name {app} AND t.tid = p.pid
            AND s.name GLOB 'Choreographer#doFrame [0-9]*' AND s.dur > 0""")]
    inputs = [(int(r["ts"]), r["name"]) for r in rows(
        f"""SELECT s.ts AS ts, s.name AS name {app} AND t.tid = p.pid
            AND s.name LIKE 'deliverInputEvent%'""")]
    render_frames = []
    for r in rows(f"""SELECT s.ts AS ts, s.dur AS dur, s.name AS name {app}
                      AND t.name = 'RenderThread' AND s.name GLOB 'DrawFrame*' AND s.dur > 0"""):
        parts = r["name"].split()
        render_frames.append((int(r["ts"]), int(r["dur"]),
                              int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else None))
    gpu_waits = [(int(r["ts"]), int(r["dur"])) for r in rows(
        f"""SELECT s.ts AS ts, s.dur AS dur {app} AND t.name = 'GPU completion'
            AND s.name GLOB 'waiting for GPU completion*' AND s.dur > 0""")]

    def timeline_rows(table, columns):
        out = {}
        for r in rows(f"""SELECT a.ts AS ts, a.dur AS dur, a.name AS name, {columns}
                          FROM {table} a JOIN process p USING (upid) WHERE p.name = '{package}'"""):
            if not r.get("name", "").isdigit() or not r.get("dur", "").isdigit():
                continue
            vsync = int(r["name"])
            value = (int(r["ts"]), int(r["dur"]), r.get("jank", ""), r.get("present", ""))
            # Several layers' rows for one vsync: the last present.
            if vsync not in out or sum(value[:2]) > sum(out[vsync][:2]):
                out[vsync] = value
        return out

    timeline = timeline_rows("actual_frame_timeline_slice",
                             "a.jank_type AS jank, a.present_type AS present")
    expected = timeline_rows("expected_frame_timeline_slice", "'' AS jank, '' AS present")
    offset = monotonic_offset(rows(
        "SELECT snapshot_id, clock_name, clock_value FROM clock_snapshot"))
    records = navigation_records(markers, frames, inputs, render_frames, gpu_waits, timeline,
                                 expected, offset)
    # Each CPU's frequencies, for the frames' work: the locked ones at locked clocks, whose
    # traces hold no frequency event.
    frequencies = {}
    locked = check_trace_clocks(trace_processor, trace, f"{package}'s trace {trace}")
    if locked is not None:
        frequencies = {cpu: [(0, 2**62, float(khz))] for cpu, khz in fixed_frequencies(
            LOCKED["expected"], LOCKED["cpus"]).items()}
    else:
        for r in rows("""INCLUDE PERFETTO MODULE linux.cpu.frequency;
                         SELECT ts, dur, cpu, freq FROM cpu_frequency_counters WHERE dur > 0"""):
            if all(r.get(key, "").isdigit() for key in ("ts", "dur", "cpu")):
                frequencies.setdefault(int(r["cpu"]), []).append(
                    (int(r["ts"]), int(r["dur"]), float(r["freq"])))
    for record in records:
        _, frame_ts, frame_dur, _ = next(f for f in frames if f[0] == record["frame_id"])
        end = frame_ts + frame_dur
        # Names may hold commas, which `query`'s rows split on.
        descendants = [(int(r["id"]), int(r["parent_id"]), r["name"], int(r["dur"])) for r in rows(
            f"""SELECT d.id AS id, d.parent_id AS parent_id, d.dur AS dur,
                       REPLACE(d.name, ',', ';') AS name
                FROM descendant_slice({record['frame_id']}) d WHERE d.dur >= 0""")
            if r.get("parent_id", "").isdigit()]
        record["phases_ms"] = frame_phases(frame_dur, descendants)
        states = [(int(r["ts"]), int(r["dur"]), r["state"],
                   int(r["cpu"]) if r.get("cpu", "").isdigit() else None)
                  for r in rows(f"""SELECT ts.ts AS ts, ts.dur AS dur, ts.state AS state,
                                           ts.cpu AS cpu
                                    FROM thread_state ts JOIN thread t USING (utid)
                                    JOIN process p USING (upid)
                                    WHERE p.name = '{package}' AND t.tid = p.pid AND ts.dur > 0
                                      AND ts.ts < {end} AND ts.ts + ts.dur > {frame_ts}""")]
        record["main_states_ms"] = overlap_by(states, frame_ts, end, lambda row: row[2])
        running = [row for row in states if row[2] == "Running" and row[3] is not None]
        record["main_cores_ms"] = overlap_by(running, frame_ts, end,
                                             lambda row: core_kind(row[3], capacities))
        record["main_work_ms"] = window_work_ms([(ts, dur, cpu) for ts, dur, _, cpu in running],
                                                frequencies, capacities, frame_ts, end)
        render = next((r for r in render_frames if r[2] == record["vsync_id"]), None)
        if render:
            running = [(int(r["ts"]), int(r["dur"]), int(r["cpu"])) for r in rows(
                f"""SELECT s.ts AS ts, s.dur AS dur, s.cpu AS cpu FROM sched s
                    JOIN thread t USING (utid) JOIN process p USING (upid)
                    WHERE p.name = '{package}' AND t.name = 'RenderThread' AND s.dur > 0
                      AND s.ts < {render[0] + render[1]} AND s.ts + s.dur > {render[0]}""")]
            record["render_cores_ms"] = overlap_by(running, render[0], render[0] + render[1],
                                                   lambda row: core_kind(row[2], capacities))
            record["render_work_ms"] = window_work_ms(running, frequencies, capacities,
                                                      render[0], render[0] + render[1])
        if record["input_to_present_ms"] is not None:
            until = record["input_ts"] + record["input_to_present_ms"] * 1e6 + 100e6
            record["other_janky_frames"] = sum(
                1 for vsync, (ts, _, jank, _) in timeline.items()
                if vsync != record["vsync_id"] and record["input_ts"] <= ts <= until
                and jank_side(jank))
    return {"markers": len(markers), "navigations": records, "locked_clocks": locked,
            # As the other traced runs' (`traced`): a trace with none is run again.
            "frames": len(records)}


def distribution(values):
    """The count, p50, p90 and max of `values`, None left out."""
    values = [value for value in values if value is not None]
    return {"count": len(values), "p50": percentile(values, 0.5), "p90": percentile(values, 0.9),
            "max": max(values) if values else None}


# The traced navigations' fields summarized, and the framestats' heavy frames'.
NAVIGATION_FIELDS = ("input_to_present_ms", "down_to_present_ms", "down_to_delivery_ms",
                     "frame_start_ms", "main_end_ms",
                     "render_end_ms", "gpu_end_ms", "do_frame_ms", "render_ms", "gpu_wait_ms",
                     "late_ms", "main_work_ms", "render_work_ms")
FRAMESTATS_FIELDS = ("frame_ms", "start_delay_ms", "animation_ms", "traversal_ms", "draw_ms",
                     "sync_ms", "render_ms", "gpu_completed_ms", "present_ms")


def navigation_summary(traced_runs, untraced_runs):
    """A subject's navigations over its runs: each traced field's and main-thread phase's
    distribution, its janky heavy frames and the inputs placed by their own time, and the
    framestats fields' of the untraced runs and of all runs."""
    records = [record for run in traced_runs for record in run["trace"]["navigations"]]
    phases = sorted({name for record in records for name in record.get("phases_ms", {})})

    def heavy_frames(runs):
        return [frame for run in runs for frame in run["framestats"] if frame]

    return {
        "traced": {field: distribution([record.get(field) for record in records])
                   for field in NAVIGATION_FIELDS},
        "phases_ms": {name: distribution([record.get("phases_ms", {}).get(name, 0.0)
                                          for record in records]) for name in phases},
        "main_cores_ms": {kind: distribution([record.get("main_cores_ms", {}).get(kind, 0.0)
                                              for record in records])
                          for kind in ("little", "mid", "big")},
        "render_cores_ms": {kind: distribution([record.get("render_cores_ms", {}).get(kind, 0.0)
                                                for record in records if "render_cores_ms" in record])
                            for kind in ("little", "mid", "big")},
        "inputs": {kind: sum(record["input"] == kind for record in records)
                   for kind in ("event_time", "delivery")},
        "janky": sum(1 for record in records if jank_side(record.get("jank_type") or "None")),
        "framestats_untraced": {field: distribution([f.get(field) for f in heavy_frames(untraced_runs)])
                                for field in FRAMESTATS_FIELDS},
        "framestats_all": {field: distribution([f.get(field) for f in
                                                heavy_frames(traced_runs + untraced_runs)])
                           for field in FRAMESTATS_FIELDS},
    }


def navigate_once(package, center):
    """One navigation: a tap on the launcher's button, the frames settling, then back. Returns
    the heavy frame from framestats (`framestats_heavy_frame`: the frames after the tap's)."""
    before = latest_intended_vsync(framestats_frames(shell(f"dumpsys gfxinfo {package} framestats")))
    shell(f"input tap {center[0]} {center[1]}")
    time.sleep(NAVIGATION_SETTLE_S)
    heavy = framestats_heavy_frame(
        framestats_frames(shell(f"dumpsys gfxinfo {package} framestats")), before)
    shell("input keyevent KEYCODE_BACK")
    time.sleep(NAVIGATION_BACK_S)
    return heavy


def navigation_run(trace_processor, out_dir, package, name, run, capacities, center, traced_run,
                   navigations, optimizations=None):
    """One run: the app started on its launcher, a warm-up navigation left out, then
    `navigations` navigations, traced or not. Returns each heavy frame's framestats, the trace's
    navigations if traced, and the thermal states."""
    thermal = cool_down(f"navigation {name} {run}")
    fresh(package)
    time.sleep(1.0)
    start(package, "navigation", optimizations=optimizations)
    time.sleep(3)
    navigate_once(package, center)
    stats = {"traced": traced_run}
    if traced_run:
        duration_ms = int(1000 * (navigations * (NAVIGATION_SETTLE_S + NAVIGATION_BACK_S
                                                 + NAVIGATION_OVERHEAD_S) + 3))
        process, path = start_trace(package, duration_ms, f"navigation-{name}-{run}",
                                    NAVIGATION_TRACE_BUFFER_KB)
    began = time.time()
    stats["framestats"] = [navigate_once(package, center) for _ in range(navigations)]
    stats["seconds"] = round(time.time() - began, 1)
    if traced_run:
        process.wait(timeout=duration_ms / 1000 + 60)
        trace = finish_trace(process, path, out_dir)
        stats["trace"] = navigation_trace(trace_processor, trace, package, capacities)
        if stats["trace"]["frames"] < navigations:
            print(f"navigation {name} {run}: {stats['trace']['frames']} of {navigations} "
                  f"navigations found in the trace ({stats['trace']['markers']} markers)")
    stats["thermal"] = finish_thermal(thermal)
    return stats


def navigation_scenario(tp, out_dir, capacities, subjects, runs, traced_runs, navigations):
    """`traced_runs` traced runs, then `runs` untraced ones, of each subject, interleaved, each of
    `navigations` navigations after a warm-up (docs/17); and each subject's summary."""
    center = screen_center(shell("wm size"))
    if center is None:
        raise SystemExit("navigation: no screen size from `wm size`")
    result = {"tap": list(center), "navigations_per_run": navigations,
              "subjects": {name: {"traced": [], "untraced": []} for name, _, _ in subjects}}
    for run in range(traced_runs + runs):
        for name, package, optimizations in subjects:
            traced_run = run < traced_runs

            def attempt():
                return navigation_run(tp, out_dir, package, name, run, capacities, center,
                                      traced_run, navigations, optimizations)

            stats = traced(attempt, f"navigation {name} {run}") if traced_run else attempt()
            result["subjects"][name]["traced" if traced_run else "untraced"].append(stats)
            print("navigation", name, run, json.dumps({
                "framestats_ms": [frame and frame["frame_ms"] for frame in stats["framestats"]],
                "input_to_present_ms": [record["input_to_present_ms"] for record in
                                        stats.get("trace", {}).get("navigations", [])]}))
    for subject in result["subjects"].values():
        subject["summary"] = navigation_summary(subject["traced"], subject["untraced"])
    return result


# ---- Counters: instructions and cycles per thread (--counters, D660) -----------------------------

# What simpleperf counts by default: instructions split by mode, and cycles. Three events: the
# Pixel 8 Pro's Cortex-X3 (cpu 8) has 3 free counters, and a fourth (cycles split by mode too)
# made them take turns there: the RenderThread's were counted 77 to 85 % of its running time.
COUNTER_EVENTS = ("instructions:u", "instructions:k", "cpu-cycles")
# The screen each counted scenario shows (the timing runs' screens).
COUNTER_SCREENS = {"scroll": "feed", "animation": "feed_animated", "stress_scroll": "stress",
                   "stress_animation": "stress_animation", "navigation": "navigation"}
# A cold start can't be counted per thread: simpleperf attaches to a process that exists (with
# --app, after its first threads ran and started others, whose counts it adds to their creator's).
COUNTER_SKIPPED = {
    "startup": "simpleperf stat attaches to a running process: a cold start's first work and "
               "threads are missed",
    "stress_startup": "as startup"}
# The thread groups, in the report's order (`counter_group`), then the process's total.
COUNTER_GROUPS = ("main", "RenderThread", "hwuiTask", "GPU driver", "binder", "JIT", "GC",
                  "ART daemons", "other")
ART_DAEMONS = ("ReferenceQueueD", "FinalizerDaemon", "FinalizerWatchd", "Signal Catcher",
               "Profile Saver", "ADB-JDWP Connec", "perfetto_hprof_", "Runtime worker")
# A counter that ran for less of its thread's running time than this was multiplexed.
MULTIPLEXED_BELOW = 0.99
# Threads that ran less than this aren't checked for multiplexing (too short to tell).
MULTIPLEX_CHECK_MIN_NS = 10_000_000
# The animations' counted window (the timing runs' traces are as long).
COUNTER_ANIMATION_S = 10
COUNTER_CSV = "/data/local/tmp/rs-compose-counters.csv"
COUNTER_GFX = "/data/local/tmp/rs-compose-counters-gfxinfo.txt"


def event_base(event):
    """An event's name without its modifier (`instructions` for `instructions:u`)."""
    return event.split(":", 1)[0]


def parse_simpleperf_stat(text, events):
    """`simpleperf stat --per-thread --csv --verbose`'s output for `events` (in the order given
    to -e): each thread's {"tid", "pid", "name", "counts": {event: n}, "running": {event: the
    share of its running time the counter ran, 1 unless multiplexed}, "enabled_ns"}, the counting
    window's "seconds", and the "mismatches" between the summary and the per-CPU rows.

    The per-CPU rows name the event without its modifier, so the n-th row of a thread and CPU
    named `instructions` is the n-th of `events` of that name. The summary's counts are those
    rows' sums (simpleperf doesn't scale them)."""
    named = {}
    for event in events:
        named.setdefault(event_base(event), []).append(event)
    seen = {}
    by_cpu = {}
    summary = {}
    seconds = None
    for line in text.splitlines():
        cells = line.strip().rstrip(",").split(",")
        if len(cells) >= 11 and cells[1] == "tid" and cells[3] == "cpu" and cells[5] == "count":
            names = named.get(cells[0])
            if not names:
                continue
            tid, cpu = int(cells[2]), int(cells[4])
            index = seen.get((cells[0], tid, cpu), 0)
            seen[(cells[0], tid, cpu)] = index + 1
            if index >= len(names):
                continue
            row = by_cpu.setdefault((tid, names[index]), [0, 0, 0])
            row[0] += int(cells[6])
            row[1] = max(row[1], int(cells[8]))
            row[2] += int(cells[10])
        elif cells[0] == "Total test time" and len(cells) >= 2:
            seconds = float(cells[1])
        elif (len(cells) >= 7 and cells[-6].isdigit() and cells[-5].isdigit()
              and cells[-4].isdigit() and cells[-3] in events):
            tid = int(cells[-5])
            thread = summary.setdefault(tid, {"tid": tid, "pid": int(cells[-6]),
                                              "name": ",".join(cells[:-6]), "counts": {}})
            thread["counts"][cells[-3]] = int(cells[-4])
    mismatches = []
    threads = []
    for tid, thread in sorted(summary.items()):
        thread["counts"] = {event: thread["counts"].get(event, 0) for event in events}
        thread["running"] = {}
        thread["enabled_ns"] = 0
        for event in events:
            count, enabled, running = by_cpu.get((tid, event), (None, 0, 0))
            thread["running"][event] = running / enabled if enabled else None
            thread["enabled_ns"] = max(thread["enabled_ns"], enabled)
            if count is not None and count != thread["counts"][event]:
                mismatches.append(f"tid {tid} {event}: {thread['counts'][event]} in the summary, "
                                  f"{count} by CPU")
        threads.append(thread)
    for (tid, event), (count, _, _) in sorted(by_cpu.items()):
        if tid not in summary and count:
            mismatches.append(f"tid {tid} {event}: {count} by CPU, not in the summary")
    return {"seconds": seconds, "threads": threads, "mismatches": mismatches}


def counter_group(name, tid, pid):
    """A thread's group in the report: the main thread (the process's own tid), HWUI's
    RenderThread and hwuiTask workers, the GPU driver's threads, binder threads, ART's JIT, GC
    and other daemons, or other (named in the report). compose-rs starts no thread of its own."""
    if tid == pid:
        return "main"
    if name == "RenderThread":
        return "RenderThread"
    if name.startswith("hwuiTask"):
        return "hwuiTask"
    if name.startswith("mali-") or name == "GPU completion":
        return "GPU driver"
    if name.startswith("binder:") or name.startswith("HwBinder:"):
        return "binder"
    if name.startswith("Jit thread pool"):
        return "JIT"
    if name == "HeapTaskDaemon":
        return "GC"
    if name.startswith(ART_DAEMONS):
        return "ART daemons"
    return "other"


def group_counters(threads, events):
    """`parse_simpleperf_stat`'s threads summed by group (`counter_group`), in COUNTER_GROUPS'
    order, then "total": each group's "counts" and its "threads" (name: how many). The main
    thread's and the RenderThread's groups are always there."""
    groups = {name: {"threads": {}, "counts": {event: 0 for event in events}}
              for name in COUNTER_GROUPS}
    total = {"threads": {}, "counts": {event: 0 for event in events}}
    for thread in threads:
        group = groups[counter_group(thread["name"], thread["tid"], thread["pid"])]
        for target in (group, total):
            target["threads"][thread["name"]] = target["threads"].get(thread["name"], 0) + 1
            for event in events:
                target["counts"][event] += thread["counts"].get(event, 0)
    out = {name: group for name, group in groups.items()
           if group["threads"] or name in ("main", "RenderThread")}
    out["total"] = total
    return out


def counter_metrics(counts, frames):
    """A group's instructions and cycles (an event of no modifier is the total; else its `:u`
    and `:k` summed), IPC, the kernel's share of instructions, and each per frame."""
    def total_of(base):
        if base in counts:
            return counts[base]
        parts = [value for event, value in counts.items() if event_base(event) == base]
        return sum(parts) if parts else None

    instructions = total_of("instructions")
    cycles = total_of("cpu-cycles")
    user, kernel = counts.get("instructions:u"), counts.get("instructions:k")
    return {
        "instructions": instructions, "instructions_u": user, "instructions_k": kernel,
        "cycles": cycles,
        "ipc": instructions / cycles if instructions is not None and cycles else None,
        "kernel_share": kernel / instructions
        if user is not None and kernel is not None and instructions else None,
        "instructions_per_frame": instructions / frames
        if frames and instructions is not None else None,
        "cycles_per_frame": cycles / frames if frames and cycles is not None else None,
        "per_frame": {event: value / frames for event, value in counts.items()} if frames else {},
    }


def counter_run_stats(parsed, frames, events):
    """A counted run: each group's counts and metrics (`counter_metrics`), the lowest share of
    a thread's running time a counter ran (`running_ratio`), and its problems: no frame, no
    instruction on the main thread or the RenderThread, multiplexed counters, mismatches."""
    groups = {}
    for name, group in group_counters(parsed["threads"], events).items():
        groups[name] = {**group, **counter_metrics(group["counts"], frames)}
    lowest = {"min": None, "where": None}
    for thread in parsed["threads"]:
        if thread.get("enabled_ns", MULTIPLEX_CHECK_MIN_NS) < MULTIPLEX_CHECK_MIN_NS:
            continue
        for event, ratio in thread["running"].items():
            if ratio is not None and (lowest["min"] is None or ratio < lowest["min"]):
                lowest = {"min": ratio, "where": f"{thread['name']} {event}"}
    problems = []
    if not frames:
        problems.append("no frame drawn")
    for name in ("main", "RenderThread"):
        if not groups[name]["instructions"]:
            problems.append(f"{name}: no instruction counted")
    if lowest["min"] is not None and lowest["min"] < MULTIPLEXED_BELOW:
        problems.append(f"counters multiplexed: {lowest['where']} counted "
                        f"{100 * lowest['min']:.1f}% of its running time")
    problems.extend(parsed["mismatches"])
    return {"frames": frames, "seconds": parsed["seconds"], "groups": groups,
            "running_ratio": lowest, "problems": problems}


def run_spread(values):
    """The median, extremes, spread ((max − min) / median) and coefficient of variation of
    runs' values, None left out."""
    values = [value for value in values if value is not None]
    if not values:
        return {"median": None, "min": None, "max": None, "spread": None, "cv": None, "runs": 0}
    median = statistics.median(values)
    mean = statistics.mean(values)
    return {"median": median, "min": min(values), "max": max(values),
            "spread": (max(values) - min(values)) / median if median else None,
            "cv": statistics.stdev(values) / mean if len(values) > 1 and mean else None,
            "runs": len(values)}


def counters_summary(runs):
    """A subject's counted runs: frames, seconds and each group's metrics over the runs
    (`run_spread`), the threads seen in each group, the lowest running ratio, and the runs'
    problems."""
    names = []
    for run in runs:
        names.extend(name for name in run["groups"] if name not in names)
    groups = {}
    for name in names:
        present = [run["groups"][name] for run in runs if name in run["groups"]]
        metrics = [key for key, value in present[0].items()
                   if key not in ("threads", "counts", "per_frame")
                   and (value is None or isinstance(value, (int, float)))]
        threads = {}
        for group in present:
            for thread, count in group["threads"].items():
                threads[thread] = max(threads.get(thread, 0), count)
        groups[name] = {metric: run_spread([run["groups"].get(name, {}).get(metric)
                                            for run in runs]) for metric in metrics}
        groups[name]["threads"] = threads
    ratios = [run["running_ratio"]["min"] for run in runs
              if run["running_ratio"]["min"] is not None]
    return {"runs": len(runs), "frames": run_spread([run["frames"] for run in runs]),
            "seconds": run_spread([run["seconds"] for run in runs]), "groups": groups,
            "running_ratio_min": min(ratios) if ratios else None,
            "problems": [f"run {index}: {problem}" for index, run in enumerate(runs)
                         for problem in run["problems"]]}


def counter_command(pid, package, events, out, gfx, duration_s=None):
    """The device's command: simpleperf counting `pid`'s threads (`events`, per thread, CSV
    with each CPU's rows into `out`) while its workload runs. The workload resets gfxinfo, says
    `READY <ns>` (`START <ns>` before the reset), waits (`duration_s`, or a line on its input),
    then writes gfxinfo to `gfx` and says `END <ns before> <ns after>`: the frames it counts were
    drawn inside the counting window, which starts before the workload and ends after it."""
    wait = f"sleep {duration_s};" if duration_s else "read line;"
    workload = (f"echo START $(date +%s%N); dumpsys gfxinfo {package} reset > /dev/null; "
                f"echo READY $(date +%s%N); {wait} "
                f"T=$(date +%s%N); dumpsys gfxinfo {package} > {gfx}; "
                f"echo END $T $(date +%s%N)")
    return (f"simpleperf stat -p {pid} --per-thread -e {','.join(events)} --csv --verbose "
            f"-o {out} sh -c '{workload}'")


def workload_window(output):
    """The workload's times (`counter_command`): before and after gfxinfo's reset (start,
    ready) and read (stop, end), in ns, and the frames' window (s): from the middle of the reset's
    dumpsys to the middle of the read's, each of which happened inside its dumpsys. None if it
    didn't end."""
    found = [re.search(pattern, output, re.M)
             for pattern in (r"^START (\d+)", r"^READY (\d+)", r"^END (\d+) (\d+)")]
    if not all(found):
        return None
    start, ready, end = found
    times = {"start_ns": int(start.group(1)), "ready_ns": int(ready.group(1)),
             "stop_ns": int(end.group(1)), "end_ns": int(end.group(2))}
    times["frame_window_s"] = round(((times["stop_ns"] + times["end_ns"])
                                     - (times["start_ns"] + times["ready_ns"])) / 2e9, 6)
    return times


def frames_in_window(frames, seconds, window, continuous):
    """The frames drawn in the counting window (simpleperf's `seconds`): gfxinfo's `frames`,
    scaled from the frames' window to it when the screen draws all the time (`continuous`: the
    animations), since the counting window holds a little more on either side (simpleperf starts
    counting before its workload, which resets gfxinfo in a dumpsys of 50 to 250 ms). A scroll's
    or a navigation's window starts and ends idle."""
    if not continuous or not seconds or not window or not window.get("frame_window_s"):
        return frames
    return round(frames * seconds / window["frame_window_s"], 1)


def hw_counters(output):
    """Each CPU's free hardware counters (`simpleperf stat --print-hw-counter`)."""
    return {int(cpu): int(count) for count, cpu in re.findall(
        r"There are (\d+) CPU PMU hardware counters available on cpu (\d+)", output)}


def counter_scenarios(scenarios):
    """The scenarios asked for that can be counted, and those skipped, with why."""
    return ([name for name in scenarios if name in COUNTER_SCREENS],
            {name: COUNTER_SKIPPED.get(name, "not countable") for name in scenarios
             if name not in COUNTER_SCREENS})


def pid_of(package):
    pids = shell(f"pidof {package}").split()
    if len(pids) != 1:
        raise SystemExit(f"{package}: {len(pids)} processes running, not one: {pids}")
    return int(pids[0])


def count_while(package, events, body=None, duration_s=None):
    """simpleperf counting `package`'s threads (`counter_command`) while `body()` runs on the
    host, or for `duration_s`. Returns the parsed counts, the frames drawn in the window
    (gfxinfo), the gfxinfo values and the workload's window."""
    shell(f"rm -f {COUNTER_CSV} {COUNTER_GFX}")
    # A first dumpsys outside the window: the reset's inside it is the quicker for it.
    shell(f"dumpsys gfxinfo {package} > /dev/null")
    command = counter_command(pid_of(package), package, events, COUNTER_CSV, COUNTER_GFX,
                              duration_s)
    process = subprocess.Popen(["adb", "shell", command], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    ready = ""
    while not ready.endswith("\n") or not ready.splitlines()[-1].startswith(("READY", "END")):
        line = process.stdout.readline()
        if not line:
            break
        ready += line
    if "READY" not in ready:
        process.kill()
        _, error = process.communicate()
        raise SystemExit(f"simpleperf didn't start counting {package}: {ready}{error}")
    if body is not None:
        body()
    if duration_s is None:
        process.stdin.write("stop\n")
        process.stdin.flush()
    output, error = process.communicate(timeout=(duration_s or 0) + 120)
    window = workload_window(ready + output)
    if process.returncode != 0 or window is None:
        raise SystemExit(f"simpleperf on {package} failed ({process.returncode}): {error}")
    parsed = parse_simpleperf_stat(shell(f"cat {COUNTER_CSV}"), events)
    gfx = parse_gfxinfo(shell(f"cat {COUNTER_GFX}"))
    shell(f"rm -f {COUNTER_CSV} {COUNTER_GFX}")
    window["simpleperf_stderr"] = error.strip()[-500:]
    return parsed, gfx.get("frames", 0), gfx, window


def navigate_plain(center):
    """A navigation without framestats (whose dumps would run in the app): tap, settle, back."""
    shell(f"input tap {center[0]} {center[1]}")
    time.sleep(NAVIGATION_SETTLE_S)
    shell("input keyevent KEYCODE_BACK")
    time.sleep(NAVIGATION_BACK_S)


def counted_run(scenario, package, name, run, events, optimizations=None, center=None,
                navigations=NAVIGATIONS):
    """One counted run of `scenario`, as its timing runs but untraced and without meminfo or
    framestats (both run code in the app): the scrolls' 10 flings after a warm-up swipe, the
    animations' 10 s, the navigations after a warm-up one. Returns `counter_run_stats` with the
    window, gfxinfo and thermal states."""
    thermal = cool_down(f"counters {scenario} {name} {run}")
    fresh(package)
    time.sleep(1.0)
    start(package, COUNTER_SCREENS[scenario], optimizations=optimizations)
    if scenario in ("scroll", "stress_scroll"):
        time.sleep(5)
        swipe(up=True)
        time.sleep(1.2)

        def flings():
            for flick in range(10):
                swipe(up=flick % 4 != 3)
                time.sleep(1.2)

        parsed, frames, gfx, window = count_while(package, events, flings)
    elif scenario in ("animation", "stress_animation"):
        time.sleep(3)
        parsed, frames, gfx, window = count_while(package, events,
                                                  duration_s=COUNTER_ANIMATION_S)
        frames = frames_in_window(frames, parsed["seconds"], window, continuous=True)
    else:
        time.sleep(3)
        navigate_plain(center)

        def navigate():
            for _ in range(navigations):
                navigate_plain(center)

        parsed, frames, gfx, window = count_while(package, events, navigate)
    stats = counter_run_stats(parsed, frames, events)
    if scenario == "navigation":
        stats["navigations"] = navigations
        for group in stats["groups"].values():
            group["instructions_per_navigation"] = (
                group["instructions"] / navigations if group["instructions"] is not None else None)
    stats["threads"] = parsed["threads"]
    stats["gfxinfo"] = gfx
    stats["window"] = window
    stats["thermal"] = finish_thermal(thermal)
    return stats


def counters_line(stats):
    """A counted run's main thread, RenderThread and total, in millions per frame."""
    def text(group):
        value = stats["groups"].get(group, {}).get("instructions_per_frame")
        return f"{group} {value / 1e6:.2f}" if value is not None else f"{group} n/a"
    return (f"{stats['frames']} frames, M instructions/frame: "
            + ", ".join(text(group) for group in ("main", "RenderThread", "total"))
            + (f"; problems {stats['problems']}" if stats["problems"] else ""))


def counters_scenario(scenario, subjects, runs, events, navigations=NAVIGATIONS, center=None):
    """`runs` counted runs of each subject, interleaved, and each subject's summary."""
    result = {"subjects": {name: {"runs": []} for name, _, _ in subjects}}
    for run in range(runs):
        for name, package, optimizations in subjects:
            stats = counted_run(scenario, package, name, run, events, optimizations, center,
                                navigations)
            result["subjects"][name]["runs"].append(stats)
            print("counters", scenario, name, run, counters_line(stats))
    for subject in result["subjects"].values():
        subject["summary"] = counters_summary(subject["runs"])
    return result


def run_counters(arguments, scenarios, subjects, report):
    """--counters: the counted runs of each countable scenario asked for, into `report`'s
    "counters" (written after each scenario)."""
    events = [event.strip() for event in arguments.counter_events.split(",") if event.strip()]
    kept, skipped = counter_scenarios(scenarios)
    for name, why in skipped.items():
        print(f"counters: {name} skipped: {why}")
    pmu = hw_counters(shell("simpleperf stat --print-hw-counter 2>&1"))
    if pmu and len(events) > min(pmu.values()):
        print(f"counters: WARNING: {len(events)} events, and a CPU has {min(pmu.values())} "
              "counters: they may be multiplexed (see each run's running_ratio)")
    version = shell("simpleperf --version 2>&1").strip()
    report["counters"] = {"tool": version.split("] ", 1)[-1], "events": events,
                          "runs": arguments.counter_runs, "skipped": skipped,
                          "pmu_counters": {str(cpu): count for cpu, count in pmu.items()},
                          "animation_s": COUNTER_ANIMATION_S, "scenarios": {}}
    center = screen_center(shell("wm size")) if "navigation" in kept else None
    for scenario in kept:
        report["counters"]["scenarios"][scenario] = counters_scenario(
            scenario, subjects, arguments.counter_runs, events, arguments.navigations, center)
        write_report(arguments.out, report)


def configure(arguments, source):
    """The mode and subjects `arguments` ask for (rotated by --rotate), each subject's full bit
    set (None: the Compose app), and the thermal guards' limits set."""
    table = optimization_table(source)
    names = [name for name, _, _ in table]
    recommended = optimization_recommended(source)
    given = getattr(arguments, "subject", None)
    if given:
        if (arguments.optimizations or "parity") != "parity" or arguments.each_only:
            raise SystemExit("--subject replaces --optimizations and --each-only")
        mode, subjects = parse_subjects(given, names)
    else:
        mode, subjects = parse_optimizations(arguments.optimizations, names, arguments.each_only)
    THERMAL["battery_limit_c"] = arguments.battery_limit
    THERMAL["wait_limit_s"] = arguments.thermal_wait_limit
    THERMAL["skin_limit_c"] = arguments.skin_limit
    rotate = arguments.rotate % len(subjects)
    subjects = subjects[rotate:] + subjects[:rotate]
    bits = {name: None if package == APPS["compose"]
            else optimization_bits(listed, table, recommended)
            for name, package, listed in subjects}
    return mode, subjects, bits


def main(argv=None):
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--trace-processor", default=None,
                        help="trace_processor_shell (required but with --counters)")
    parser.add_argument("--out", required=True)
    parser.add_argument("--runs", type=int, default=10)
    parser.add_argument("--compile", choices=["speed", "speed-profile", "verify"],
                        default="speed", help="the apps' ART compilation filter (see above)")
    parser.add_argument("--allow-stale", action="store_true",
                        help="measure even if the installed APKs aren't the ones built here")
    parser.add_argument("--scenarios", default="all",
                        help="the scenarios to run, comma-separated (see above; default all)")
    parser.add_argument("--optimizations", default="parity",
                        help="compose-rs's optimizations: parity (default), recommended, all, "
                             "each, or names (see above)")
    parser.add_argument("--subject", action="append", default=None, metavar="[NAME=]LIST",
                        help="a bench-rs subject with this list, beside Compose; repeated, several "
                             "(see above)")
    parser.add_argument("--traced-runs", type=int, default=3,
                        help="traced runs of each subject in the scroll and animation scenarios")
    parser.add_argument("--battery-limit", type=float, default=THERMAL["battery_limit_c"],
                        help="the battery temperature (°C) above which a run waits")
    parser.add_argument("--thermal-wait-limit", type=int, default=THERMAL["wait_limit_s"],
                        help="the most a run waits for the device to cool (s; 0: no limit)")
    parser.add_argument("--skin-limit", type=float, default=None,
                        help="the skin temperature (°C, the highest skin sensor) above which a run waits "
                             "(none by default)")
    parser.add_argument("--rotate", type=int, default=0,
                        help="start the subjects' order this many places on (each run's order "
                             "the same), so no subject always runs warmest")
    parser.add_argument("--navigations", type=int, default=NAVIGATIONS,
                        help="navigations per run in the navigation scenario (after a warm-up)")
    parser.add_argument("--each-only", default=None,
                        help="with --optimizations each: only these optimizations, "
                             "comma-separated")
    parser.add_argument("--locked-clocks", default=None, metavar="STATE",
                        help="lock_clocks.sh's state file: check before and after each run, "
                             "and in each trace, that the locked clocks hold (see above)")
    parser.add_argument("--fixed-placement", action="store_true",
                        help="with --locked-clocks: the state also locks the top-app group's "
                             "uclamp.min (lock_clocks.sh's placement=), checked as the clocks")
    parser.add_argument("--counters", action="store_true",
                        help="count instructions and cycles per thread with simpleperf instead "
                             "of the timing and traced runs (see above)")
    parser.add_argument("--counter-runs", type=int, default=15,
                        help="with --counters: counted runs of each subject in each scenario")
    parser.add_argument("--counter-events", default=",".join(COUNTER_EVENTS),
                        help="with --counters: simpleperf's events, comma-separated")
    arguments = parser.parse_args(argv)
    if not arguments.counters and not arguments.trace_processor:
        parser.error("--trace-processor is required (but with --counters)")
    scenarios = parse_scenarios(arguments.scenarios)
    with open(OPTIMIZATIONS_SOURCE) as file:
        source = file.read()
    mode, subjects, bits = configure(arguments, source)
    thermal_log = []
    os.makedirs(arguments.out, exist_ok=True)
    tp = arguments.trace_processor
    capacities = cpu_capacities()
    report = {"device": shell("getprop ro.product.model").strip(),
              "android": shell("getprop ro.build.version.release").strip(),
              "abi": shell("getprop ro.product.cpu.abi").strip(),
              "compile": arguments.compile,
              "scenarios": scenarios,
              # bench-rs's `-e optimizations` for each subject (None: the Compose app).
              "optimizations": {"mode": mode,
                                "subjects": {name: given for name, _, given in subjects},
                                # Each subject's bits, Rust's table's (`Optimization::ALL`).
                                "bits": bits},
              "thermal": {"battery_limit_c": THERMAL["battery_limit_c"],
                          "skin_limit_c": THERMAL["skin_limit_c"],
                          "wait_limit_s": THERMAL["wait_limit_s"], "startup": thermal_log},
              # Each CPU's capacity and highest frequency (kHz): what "work" and the little
              # cores' share are computed from.
              "cpu_capacities": {str(cpu): list(value) for cpu, value in capacities.items()}}
    report["apks"] = check_apks(arguments.out, report["abi"], arguments.allow_stale)
    report["target_sdk"] = {name: target_sdk(shell(f"dumpsys package {package}"))
                            for name, package in APPS.items()}
    if len(set(report["target_sdk"].values())) > 1:
        print(f"warning: the apps target different SDKs: {report['target_sdk']}")
    LOCKED["expected"] = load_locked_clocks(arguments.locked_clocks)
    report["fixed_placement"] = check_placement(LOCKED["expected"], arguments.fixed_placement)
    if LOCKED["expected"] is not None:
        start_clocks = read_clocks()
        LOCKED["cpus"] = {name: value["cpus"] for name, value in start_clocks.items()
                          if name.startswith("policy")}
        report["locked_clocks"] = {"expected": LOCKED["expected"], "cpus": LOCKED["cpus"],
                                   "start": {name: value for name, value in start_clocks.items()
                                             if name != "residency"}}
        check_clocks("start")

    shell("input keyevent KEYCODE_WAKEUP")
    shell("wm dismiss-keyguard")
    compile_apps(arguments.compile)
    report["dexopt"] = {"start": log_dexopt("start")}
    check_compiled(report["dexopt"]["start"], arguments.compile)

    report["counters_mode"] = arguments.counters
    try:
        if arguments.counters:
            run_counters(arguments, scenarios, subjects, report)
        else:
            run_scenarios(arguments, scenarios, tp, capacities, subjects, thermal_log, report)
    except ClocksMoved as moved:
        # What ran before is kept; the rest didn't run at the locked clocks.
        report["locked_clocks"]["moved"] = str(moved)
        write_report(arguments.out, report)
        raise SystemExit(f"stopped: {moved}")
    if LOCKED["expected"] is not None:
        report["locked_clocks"]["end"] = {name: value for name, value in
                                          check_clocks("end").items() if name != "residency"}
    for package in APPS.values():
        stop(package)
    # A background dexopt job can recompile an app during a run.
    report["dexopt"]["end"] = log_dexopt("end")
    write_report(arguments.out, report)


def write_report(out, report):
    with open(os.path.join(out, "report.json"), "w") as file:
        json.dump(report, file, indent=1)
    print("wrote", os.path.join(out, "report.json"))


def run_scenarios(arguments, scenarios, tp, capacities, subjects, thermal_log, report):
    """Each scenario asked for, in order, into `report`."""
    traced_runs = arguments.traced_runs
    if "startup" in scenarios:
        report["startup_ms"], report["first_frame"] = startup_scenario(
            tp, arguments.out, arguments.runs, ("feed", "big"), subjects, thermal_log)
    if "scroll" in scenarios:
        report["memory_kb"], report["scroll"] = scroll_scenario(
            tp, arguments.out, capacities, "feed", "scroll", subjects, traced_runs)
    if "animation" in scenarios:
        report["animation"] = animation_scenario(
            tp, arguments.out, capacities, "feed_animated", "animation", subjects, traced_runs)
    if "stress_startup" in scenarios:
        report["stress_startup_ms"], report["stress_first_frame"] = startup_scenario(
            tp, arguments.out, arguments.runs, ("stress",), subjects, thermal_log)
    if "stress_scroll" in scenarios:
        report["stress_memory_kb"], report["stress_scroll"] = scroll_scenario(
            tp, arguments.out, capacities, "stress", "stress-scroll", subjects, traced_runs,
            STRESS_TRACE_BUFFER_KB)
    if "stress_animation" in scenarios:
        report["stress_animation"] = animation_scenario(
            tp, arguments.out, capacities, "stress_animation", "stress-animation", subjects,
            traced_runs, memory=True, buffer_kb=STRESS_TRACE_BUFFER_KB)
    if "navigation" in scenarios:
        report["navigation"] = navigation_scenario(
            tp, arguments.out, capacities, subjects, arguments.runs, traced_runs,
            arguments.navigations)


if __name__ == "__main__":
    main()
