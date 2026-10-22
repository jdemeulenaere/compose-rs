#!/usr/bin/env python3
"""Measures the benchmark apps (bench-compose: Kotlin Compose; bench-rs: compose-rs) on a
connected device, interleaving the two apps run by run, and writes a JSON report.

    measure.py --trace-processor PATH --out DIR [--runs N]

Scenarios (docs/12-android-performance.md):
  startup      cold starts of the feed and of the big screen (`am start -W`: TotalTime), and a
               Perfetto trace of a cold start of each for the first frame's breakdown
  memory       PSS and its parts (`dumpsys meminfo`) after start, and after scrolling
  scroll       10 flings through the feed: frames (`dumpsys gfxinfo`) and jank (FrameTimeline)
  animation    10 s of the animated header, no input: the same
"""

import argparse
import json
import os
import re
import statistics
import subprocess
import time

APPS = {"compose": "rs.compose.bench.compose", "rs": "rs.compose.bench.rs"}


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


def cold_start(package, screen):
    stop(package)
    time.sleep(1.0)
    output = start(package, screen)
    match = re.search(r"TotalTime: (\d+)", output)
    return int(match.group(1)) if match else None


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


def work_ms(trace_processor, trace, package, capacities):
    """The main thread's and RenderThread's work as milliseconds on the fastest core at its
    highest frequency: each running slice's time scaled by its core's frequency and capacity.
    Unlike time, it doesn't grow when the scheduler moves light work to slower cores."""
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
          WHERE p.name = '{package}' AND s.dur > 0
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


def frame_stats(trace_processor, trace, package, capacities=None):
    """FrameTimeline jank and the app's main thread and RenderThread time per frame."""
    timeline = query(
        trace_processor,
        trace,
        f"""
        SELECT a.dur AS dur, a.jank_type AS jank
        FROM actual_frame_timeline_slice a JOIN process p USING (upid)
        WHERE p.name = '{package}'
        """,
    )
    jank_kinds = {}
    for row in timeline:
        if row["jank"] not in ("None", "Buffer Stuffing"):
            jank_kinds[row["jank"]] = jank_kinds.get(row["jank"], 0) + 1
    janky = sum(jank_kinds.values())

    def durations(thread_condition, name_condition):
        rows = query(
            trace_processor,
            trace,
            f"""
            SELECT s.dur AS dur FROM slice s
            JOIN thread_track tt ON s.track_id = tt.id
            JOIN thread t USING (utid) JOIN process p USING (upid)
            WHERE p.name = '{package}' AND {thread_condition} AND {name_condition}
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
        WHERE p.name = '{package}' AND ts.state = 'Running'
          AND (t.tid = p.pid OR t.name = 'RenderThread')
        GROUP BY t.utid
        """,
    )
    cpu_ms = {"main": 0.0, "render": 0.0}
    for row in cpu:
        if row.get("running", "").isdigit():
            kind = "render" if row["thread"] == "RenderThread" else "main"
            cpu_ms[kind] += int(row["running"]) / 1e6
    return {
        "frames": len(timeline),
        "janky_frames": janky,
        "jank_kinds": jank_kinds,
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
        "work_ms": work_ms(trace_processor, trace, package, capacities),
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--trace-processor", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--runs", type=int, default=10)
    arguments = parser.parse_args()
    os.makedirs(arguments.out, exist_ok=True)
    tp = arguments.trace_processor
    capacities = cpu_capacities()
    report = {"device": shell("getprop ro.product.model").strip(),
              "android": shell("getprop ro.build.version.release").strip()}

    # Both apps fully compiled ahead of time: Compose at its best.
    for package in APPS.values():
        shell(f"cmd package compile -m speed -f {package}")
    shell("input keyevent KEYCODE_WAKEUP")
    shell("wm dismiss-keyguard")

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
            stop(package)
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
            stop(package)
            time.sleep(1.0)
            start(package, "feed")
            time.sleep(5)
            memory[name]["after_start"].append(meminfo(package))
            gfxinfo(package, reset=True)
            process, path = start_trace(package, 16000, f"scroll-{name}-{run}")
            for flick in range(10):
                swipe(up=flick % 4 != 3)
                time.sleep(1.2)
            trace = finish_trace(process, path, arguments.out)
            stats = {"gfxinfo": gfxinfo(package), "trace": frame_stats(tp, trace, package, capacities)}
            scroll[name].append(stats)
            memory[name]["after_scroll"].append(meminfo(package))
            print("scroll", name, run, json.dumps(stats))
    report["memory_kb"] = memory
    report["scroll"] = scroll

    # The animated header, untouched.
    animation = {name: [] for name in APPS}
    for run in range(3):
        for name, package in APPS.items():
            stop(package)
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
    with open(os.path.join(arguments.out, "report.json"), "w") as file:
        json.dump(report, file, indent=1)
    print("wrote", os.path.join(arguments.out, "report.json"))


if __name__ == "__main__":
    main()
