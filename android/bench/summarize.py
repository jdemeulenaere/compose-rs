"""Prints a measure.py report: python3 android/bench/summarize.py report.json"""
import json, statistics, sys

med = statistics.median


def run_line(trace):
    """One traced run's jank details (reports since D343 have them; earlier ones don't): janky
    frames by side, frames presented at another vsync than expected, late doFrames, prefetches,
    and whether Choreographer recovered from buffer stuffing."""
    if "jank_sides" not in trace:
        return None
    sides = trace["jank_sides"]
    stuffing = trace["buffer_stuffing"]
    main = trace["main_thread_frame_ms"]
    render = trace["render_thread_frame_ms"]
    return (f"janky app {sides['app']} compositor {sides['compositor']} other {sides['other']}, "
            f"visibly late {trace['visibly_late_frames']}, late doFrames {trace['late_do_frames']} (after work {trace.get('late_do_frames_after_work', '?')}), "
            f"prefetches {trace['prefetches']}, buffer stuffing "
            + (f"yes ({stuffing['recovery_slices']} recovery slices, {stuffing['buffer_stuffed']} stuffed)"
               if stuffing["occurred"] else "no")
            + f" | main p50 {main['p50'] or 0:.2f} p90 {main['p90'] or 0:.2f} p99 {main['p99'] or 0:.2f}"
            + f" | render p50 {render['p50'] or 0:.2f} p90 {render['p90'] or 0:.2f} p99 {render['p99'] or 0:.2f}")


def percent(value):
    return "n/a" if value is None else f"{100 * value:.0f}%"


def boost_line(trace):
    """One traced run's RenderThread boost picture (reports since round 6 have it): the share of
    its running time on little cores, the HWUI session's time-weighted boost minimum, the share
    of frames unboosted (a minimum under 100), and the duration percentiles per boost bucket."""
    boost = trace.get("render_boost")
    if boost is None:
        return None
    line = f"render: little cores {percent(boost['little_core_share'])} of running"
    if not boost["counter"]:
        return line + ", no HWUI boost counter in the trace"
    if boost["buckets"] is None:
        return line + ", no frame with a boost value"
    line += (f", boost min time-weighted {boost['time_weighted_min']:.0f}, frames unboosted "
             f"{percent(boost['unboosted_frame_share'])}")
    if boost["frames_before_counter"]:
        line += f" ({boost['frames_before_counter']} before the counter left out)"
    for name, bucket in boost["buckets"].items():
        line += f" | {name} {bucket['frames']} ({percent(bucket['share'])})"
        if bucket["frames"]:
            line += f" p50 {bucket['p50']:.2f} p90 {bucket['p90']:.2f} p99 {bucket['p99']:.2f}"
    return line


def boost_summary(traces):
    """The runs' medians of boost_line's numbers (of the runs with the counter, for the
    boost's; a bucket's percentiles, of the runs with frames in it), or None if no run has the
    boost picture."""
    boosts = [trace["render_boost"] for trace in traces if trace.get("render_boost")]
    if not boosts:
        return None
    little = [b["little_core_share"] for b in boosts if b["little_core_share"] is not None]
    line = f"little cores {percent(med(little)) if little else 'n/a'} of running"
    counted = [b for b in boosts if b["buckets"] is not None]
    if not counted:
        return line + ", no HWUI boost counter"
    line += (f", boost min time-weighted {med([b['time_weighted_min'] for b in counted]):.0f}, "
             f"frames unboosted {percent(med([b['unboosted_frame_share'] for b in counted]))}")
    for name in counted[0]["buckets"]:
        share = med([b["buckets"][name]["share"] for b in counted])
        line += f" | {name} {percent(share)}"
        buckets = [b["buckets"][name] for b in counted if b["buckets"][name]["frames"]]
        if buckets:
            line += (f" p50 {med([b['p50'] for b in buckets]):.2f} "
                     f"p90 {med([b['p90'] for b in buckets]):.2f} "
                     f"p99 {med([b['p99'] for b in buckets]):.2f}")
    return line


def cluster_summary(traces):
    """The runs' medians of each thread's running time on the little, mid and big cores
    (`cluster_ms`, reports since D640), or None."""
    clusters = [trace["cluster_ms"] for trace in traces if trace.get("cluster_ms")]
    if not clusters:
        return None
    return " | ".join(
        f"{thread} " + " ".join(f"{kind} {med([c[thread][kind] for c in clusters]):.0f}"
                                for kind in ("little", "mid", "big")) + " ms"
        for thread in ("main", "render"))


def slowest_line(trace, thread):
    """A traced run's slowest frame on `thread` ("main" or "render", reports since docs/15 have
    them) with its three longest slices inside, or None."""
    frames = (trace.get("slowest_frames") or {}).get(thread)
    if not frames:
        return None
    frame = frames[0]
    inside = ", ".join(f"{s['name']} {s['ms']:.2f}" for s in frame["slices"][:3])
    return f"slowest {thread} frame {frame['ms']:.2f} ms: {inside}"


def frames_summary(r, scenario):
    """The medians of a frames scenario's runs, then a line per run."""
    for app in r[scenario]:
        # A run whose trace holds no frame (measure.py runs it again since D362; older
        # reports may have one) would read as zero work: left out, and said.
        runs = [x for x in r[scenario][app] if x["trace"].get("frames", 1) > 0]
        if len(runs) < len(r[scenario][app]):
            print(f"{scenario} {app}: {len(r[scenario][app]) - len(runs)} run(s) without a traced frame left out")
        g = [x["gfxinfo"] for x in runs]; t = [x["trace"] for x in runs]
        print(f"{scenario} {app}: gfx frames {med([x.get('frames',0) for x in g])}, janky {med([x.get('janky',0) for x in g])}, p50 {med([x.get('p50',0) for x in g])} p90 {med([x.get('p90',0) for x in g])} p99 {med([x.get('p99',0) for x in g])} | timeline frames {med([x['frames'] for x in t])}, janky {med([x['janky_frames'] for x in t])} kinds {[x['jank_kinds'] for x in t]} | main p50 {med([x['main_thread_frame_ms']['p50'] or 0 for x in t]):.2f} p90 {med([x['main_thread_frame_ms']['p90'] or 0 for x in t]):.2f} p99 {med([x['main_thread_frame_ms']['p99'] or 0 for x in t]):.2f} max {med([x['main_thread_frame_ms']['max'] or 0 for x in t]):.1f} | render p50 {med([x['render_thread_frame_ms']['p50'] or 0 for x in t]):.2f} p90 {med([x['render_thread_frame_ms']['p90'] or 0 for x in t]):.2f} p99 {med([x['render_thread_frame_ms']['p99'] or 0 for x in t]):.2f} | cpu main {med([x['cpu_ms'].get('main',0) for x in t]):.0f}ms render {med([x['cpu_ms'].get('render',0) for x in t]):.0f}ms | work main {med([(x.get('work_ms') or {}).get('main',0) for x in t]):.0f}ms render {med([(x.get('work_ms') or {}).get('render',0) for x in t]):.0f}ms")
        boost = boost_summary(t)
        if boost:
            print(f"  {scenario} {app} render boost (medians): {boost}")
        clusters = cluster_summary(t)
        if clusters:
            print(f"  {scenario} {app} running by cluster (medians): {clusters}")
        for index, trace in enumerate(t):
            for line in (run_line(trace), boost_line(trace), slowest_line(trace, "main"),
                         slowest_line(trace, "render"), thermal_line(runs[index])):
                if line:
                    print(f"  {scenario} {app} run {index}: {line}")
        animation_memory = [x["memory_kb"] for x in runs if x.get("memory_kb")]
        if animation_memory:
            print_memory(f"{scenario} {app}", {
                when: [m[when] for m in animation_memory] for when in animation_memory[0]})


def thermal_line(run):
    """The thermal guard's record of a run (since D569): its state before and after, and any
    wait, or None."""
    thermal = run.get("thermal")
    if not thermal:
        return None
    def state(s):
        return f"status {s.get('status')}, battery {s.get('battery_c')} °C, skin {s.get('skin_c')} °C"
    line = f"thermal before: {state(thermal['before'])}; after: {state(thermal.get('after', {}))}"
    if thermal.get("waited_s"):
        line += f"; waited {thermal['waited_s']} s" + (" and gave up" if thermal.get("gave_up") else "")
    return line


MEMORY_KEYS = ["Total PSS", "Java Heap", "Native Heap", "Code", "Graphics", "Private Other", "System", "Total RSS"]


def print_memory(label, by_when):
    """The medians of `dumpsys meminfo`'s parts, for each moment (after start, …)."""
    for when, runs in by_when.items():
        print(f"memory {label} {when}: " + ", ".join(f"{k} {med([x.get(k,0) for x in runs])/1024:.1f}MB" for k in MEMORY_KEYS))


def startup_summary(r, key, first_key, screens):
    for app in r[key]:
        s = r[key][app]
        print(f"{key} {app}: " + ", ".join(f"{screen} median {s[screen]['median']} (min {s[screen]['min']}, max {s[screen]['max']})" for screen in screens))
    for app in r[first_key]:
        for screen in screens:
            f = r[first_key][app][screen]["first"]
            keys = [k for k in ("bindApplication","activityStart","activityResume","Choreographer#doFrame","traversal","Record View#draw()","compose-rs: frame","compose-rs: recompose","compose-rs: layout","compose-rs: text layout","compose-rs: draw","compose-rs: replay") if k in f]
            print(f"first {app} {screen}: " + ", ".join(f"{k} {f[k]:.1f}" for k in keys))


def distribution_text(d):
    """A `measure.distribution`: its p50, p90 and max, and its count."""
    if not d["count"]:
        return "none"
    return f"p50 {d['p50']:.1f} p90 {d['p90']:.1f} max {d['max']:.1f} ms ({d['count']})"


def navigation_summary(navigation):
    """The navigation scenario (docs/17): each subject's latency and frame, then its main
    thread's phases (p50), then its heavy frames' framestats."""
    for name, subject in navigation["subjects"].items():
        s = subject["summary"]
        traced = s["traced"]
        print(f"navigation {name}: input to present {distribution_text(traced['input_to_present_ms'])}; "
              f"doFrame {distribution_text(traced['do_frame_ms'])}; "
              f"DrawFrame {distribution_text(traced['render_ms'])}; "
              f"GPU done after input {distribution_text(traced['gpu_end_ms'])}; "
              f"late {distribution_text(traced['late_ms'])}; janky {s['janky']}; "
              f"from the down {distribution_text(traced['down_to_present_ms'])} "
              f"({s['inputs']['event_time']} placed)")
        print(f"navigation {name} phases p50: " + ", ".join(
            f"{phase} {d['p50']:.1f}" for phase, d in sorted(s["phases_ms"].items()) if d["count"]))
        if "main_cores_ms" in s:
            print(f"navigation {name} main thread: work {distribution_text(traced['main_work_ms'])}; "
                  + ", ".join(f"on {kind} cores p50 {d['p50']:.1f}"
                              for kind, d in s["main_cores_ms"].items() if d["count"]))
        for key, label in (("framestats_untraced", "untraced"), ("framestats_all", "all runs")):
            f = s[key]
            print(f"navigation {name} framestats ({label}): frame {distribution_text(f['frame_ms'])}; "
                  + "; ".join(f"{field[:-3]} {distribution_text(f[field])}" for field in
                              ("animation_ms", "traversal_ms", "draw_ms", "render_ms",
                               "gpu_completed_ms", "present_ms")))


def counters_lines(counters):
    """measure.py --counters (D660): for each scenario and subject, each thread group's median
    instructions per frame (millions), the runs' spread ((max - min) / median), its IPC and the
    kernel's share of its instructions; then the subject's problems, if any. The calibration
    (`idle`, the feed at rest) is in instructions per second."""
    lines = []
    for scenario, result in counters["scenarios"].items():
        idle = scenario == "idle"
        metric, unit = (("instructions_per_second", "M/s") if idle
                        else ("instructions_per_frame", "M/frame"))
        for name, subject in result["subjects"].items():
            s = subject.get("summary")
            if s is None:
                # A report written on a failure: the runs done, summarized here.
                if not subject.get("runs"):
                    continue
                import measure
                s = measure.counters_summary(subject["runs"])
            parts = []
            for group, metrics in s["groups"].items():
                per_frame = metrics.get(metric, {})
                if per_frame.get("median") is None:
                    continue
                text = f"{group} {per_frame['median'] / 1e6:.2f} {unit}"
                if per_frame.get("spread") is not None:
                    text += f" (spread {100 * per_frame['spread']:.1f}%)"
                ipc = metrics.get("ipc", {}).get("median")
                text += f" IPC {ipc:.2f}" if ipc is not None else " IPC n/a"
                kernel = metrics.get("kernel_share", {}).get("median")
                if kernel is not None:
                    text += f" kernel {100 * kernel:.0f}%"
                # A navigation's frames vary in number (40 to 47 in 20 navigations): per
                # navigation is the steadier measure there.
                navigation = metrics.get("instructions_per_navigation", {})
                if navigation.get("median") is not None:
                    text += f", {navigation['median'] / 1e6:.1f} M/navigation"
                    if navigation.get("spread") is not None:
                        text += f" (spread {100 * navigation['spread']:.1f}%)"
                parts.append(text)
            frames = s["frames"]["median"]
            if idle:
                what = "at rest"
            else:
                what = (f"{frames:.0f}" if frames is not None else "no") + " frames"
                if s.get("frame_scale_uncertainty_max") is not None:
                    what += f", frames scaled ±{100 * s['frame_scale_uncertainty_max']:.2f}%"
            lines.append(f"counters {scenario} {name} ({s['runs']} runs, {what}): "
                         + " | ".join(parts))
            if s["problems"]:
                lines.append(f"counters {scenario} {name} problems: {'; '.join(s['problems'])}")
    return lines


def main(path):
    r = json.load(open(path))
    print("device", r["device"], "android", r["android"])
    # A report measure.py wrote on a failure (or ^C): what stopped it.
    if r.get("stopped"):
        print("stopped:", r["stopped"])
    # Reports since docs/15 name the scenarios run; earlier ones ran the first three.
    print("scenarios", ", ".join(r.get("scenarios", ["startup", "scroll", "animation"])))
    if "startup_ms" in r:
        startup_summary(r, "startup_ms", "first_frame", ("feed", "big"))
    # Since D569: bench-rs's optimizations for each subject, and the thermal guards' waits.
    if "optimizations" in r:
        print("optimizations", r["optimizations"]["mode"], ", ".join(
            f"{name}: {given}" for name, given in r["optimizations"]["subjects"].items()))
    if "memory_kb" in r:
        for app in r["memory_kb"]:
            print_memory(app, r["memory_kb"][app])
    for scenario in ("scroll", "animation"):
        if scenario in r:
            frames_summary(r, scenario)
    if "stress_startup_ms" in r:
        startup_summary(r, "stress_startup_ms", "stress_first_frame", ("stress",))
    if "stress_memory_kb" in r:
        for app in r["stress_memory_kb"]:
            print_memory(f"stress {app}", r["stress_memory_kb"][app])
    for scenario in ("stress_scroll", "stress_animation"):
        if scenario in r:
            frames_summary(r, scenario)
    if "navigation" in r:
        navigation_summary(r["navigation"])
    if "counters" in r:
        print("counters", ", ".join(r["counters"]["events"]), "|", r["counters"].get("tool", ""))
        for line in counters_lines(r["counters"]):
            print(line)
    # Reports from before measure.py recorded them have no "compile", "apks" or "dexopt".
    print("compile", r.get("compile", "speed (not recorded)"))
    for app, apk in r.get("apks", {}).items():
        installed = apk["installed"]
        libraries = ", ".join(f"{name} {value['bytes']} bytes sha1 {value['sha1'][:12]}" for name, value in installed["libraries"].items())
        print(f"apk {app}: {installed['bytes']} bytes, sha1 {installed['sha1'][:12]}; {libraries}" + (f"; WARNING {apk['problems']}" if apk["problems"] else ""))
    for when, apps in r.get("dexopt", {}).items():
        for app, states in apps.items():
            print(f"dexopt {when} {app}: " + (", ".join(f"{s['isa']} {s['status']} ({s['reason']})" for s in states) or "not found"))


if __name__ == "__main__":
    main(sys.argv[1])
