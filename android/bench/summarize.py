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
            f"visibly late {trace['visibly_late_frames']}, late doFrames {trace['late_do_frames']}, "
            f"prefetches {trace['prefetches']}, buffer stuffing "
            + (f"yes ({stuffing['recovery_slices']} recovery slices, {stuffing['buffer_stuffed']} stuffed)"
               if stuffing["occurred"] else "no")
            + f" | main p50 {main['p50'] or 0:.2f} p90 {main['p90'] or 0:.2f} p99 {main['p99'] or 0:.2f}"
            + f" | render p50 {render['p50'] or 0:.2f} p90 {render['p90'] or 0:.2f} p99 {render['p99'] or 0:.2f}")


def main(path):
    r = json.load(open(path))
    print("device", r["device"], "android", r["android"])
    for app in ("compose", "rs"):
        s = r["startup_ms"][app]
        print(f"startup {app}: feed median {s['feed']['median']} (min {s['feed']['min']}, max {s['feed']['max']}), big median {s['big']['median']} (min {s['big']['min']})")
    for app in ("compose", "rs"):
        for screen in ("feed", "big"):
            f = r["first_frame"][app][screen]["first"]
            keys = [k for k in ("bindApplication","activityStart","activityResume","Choreographer#doFrame","traversal","Record View#draw()","compose-rs: frame","compose-rs: recompose","compose-rs: layout","compose-rs: text layout","compose-rs: draw","compose-rs: replay") if k in f]
            print(f"first {app} {screen}: " + ", ".join(f"{k} {f[k]:.1f}" for k in keys))
    for app in ("compose", "rs"):
        for when in ("after_start", "after_scroll"):
            runs = r["memory_kb"][app][when]
            keys = ["Total PSS", "Java Heap", "Native Heap", "Code", "Graphics", "Private Other", "System", "Total RSS"]
            print(f"memory {app} {when}: " + ", ".join(f"{k} {med([x.get(k,0) for x in runs])/1024:.1f}MB" for k in keys))
    for scenario in ("scroll", "animation"):
        for app in ("compose", "rs"):
            runs = r[scenario][app]
            g = [x["gfxinfo"] for x in runs]; t = [x["trace"] for x in runs]
            print(f"{scenario} {app}: gfx frames {med([x.get('frames',0) for x in g])}, janky {med([x.get('janky',0) for x in g])}, p50 {med([x.get('p50',0) for x in g])} p90 {med([x.get('p90',0) for x in g])} p99 {med([x.get('p99',0) for x in g])} | timeline frames {med([x['frames'] for x in t])}, janky {med([x['janky_frames'] for x in t])} kinds {[x['jank_kinds'] for x in t]} | main p50 {med([x['main_thread_frame_ms']['p50'] or 0 for x in t]):.2f} p90 {med([x['main_thread_frame_ms']['p90'] or 0 for x in t]):.2f} p99 {med([x['main_thread_frame_ms']['p99'] or 0 for x in t]):.2f} max {med([x['main_thread_frame_ms']['max'] or 0 for x in t]):.1f} | render p50 {med([x['render_thread_frame_ms']['p50'] or 0 for x in t]):.2f} p90 {med([x['render_thread_frame_ms']['p90'] or 0 for x in t]):.2f} | cpu main {med([x['cpu_ms'].get('main',0) for x in t]):.0f}ms render {med([x['cpu_ms'].get('render',0) for x in t]):.0f}ms | work main {med([(x.get('work_ms') or {}).get('main',0) for x in t]):.0f}ms render {med([(x.get('work_ms') or {}).get('render',0) for x in t]):.0f}ms")
            for index, trace in enumerate(t):
                line = run_line(trace)
                if line:
                    print(f"  {scenario} {app} run {index}: {line}")
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
