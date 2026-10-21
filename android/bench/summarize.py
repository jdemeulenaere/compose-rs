import json, statistics, sys
r = json.load(open(sys.argv[1]))
med = statistics.median
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
        print(f"{scenario} {app}: gfx frames {med([x.get('frames',0) for x in g])}, janky {med([x.get('janky',0) for x in g])}, p50 {med([x.get('p50',0) for x in g])} p90 {med([x.get('p90',0) for x in g])} p99 {med([x.get('p99',0) for x in g])} | timeline frames {med([x['frames'] for x in t])}, janky {med([x['janky_frames'] for x in t])} kinds {[x['jank_kinds'] for x in t]} | main p50 {med([x['main_thread_frame_ms']['p50'] or 0 for x in t]):.2f} p90 {med([x['main_thread_frame_ms']['p90'] or 0 for x in t]):.2f} p99 {med([x['main_thread_frame_ms']['p99'] or 0 for x in t]):.2f} max {med([x['main_thread_frame_ms']['max'] or 0 for x in t]):.1f} | render p50 {med([x['render_thread_frame_ms']['p50'] or 0 for x in t]):.2f} p90 {med([x['render_thread_frame_ms']['p90'] or 0 for x in t]):.2f} | cpu main {med([x['cpu_ms'].get('main',0) for x in t]):.0f}ms render {med([x['cpu_ms'].get('render',0) for x in t]):.0f}ms")
