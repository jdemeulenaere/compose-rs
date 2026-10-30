#!/usr/bin/env python3
"""Compares the instructions two checkouts retire on fixed work (examples/instructions.rs).

Timing on a loaded machine swings up to 4x; retired instructions move by a fraction of a percent
(docs/14, "Method"). macOS: reads `/usr/bin/time -l`. Not for CI.

    benchmarks/compare_instructions.py main                     # this checkout against a revision
    benchmarks/compare_instructions.py ../other --rounds 5      # ... or another checkout
    benchmarks/compare_instructions.py main grid_step:500       # scenario:iterations
    benchmarks/compare_instructions.py main --dumps             # the same display lists?

The base is built with this checkout's example, so both have the same scenarios; each base has its
own target directory under --scratch, which can be reused.
"""
import argparse
import hashlib
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
EXAMPLE = "instructions"
# Iterations about a second of work each.
SCENARIOS = {
    "state_read": 2000000,
    "state_write": 1000000,
    "derived_read": 1000000,
    "derived_after_write": 500000,
    "derived_recompute": 200000,
    "counter_click": 5000,
    "counter_click_touch": 5000,
    "todo_toggle": 40,
    "first_frame_feed": 200,
    "first_frame_big": 10,
    "first_frame_clickables": 10,
    "first_frame_scrollables": 30,
    "list_scroll": 3000,
    "feed_pixel": 5000,
    "feed_step": 1000,
    "stress_pixel": 3000,
    "stress_step": 500,
    "grid_pixel": 3000,
    "grid_step": 500,
    "feed_animated": 3000,
    "stress_animation": 500,
    "cells_animated": 1000,
    "cells_animated_positioned": 1000,
    "first_frame_list_items": 100,
    "first_frame_selectable_items": 100,
    "first_frame_chips": 100,
    "nav_selection": 300,
    "list_selection": 100,
    "field_focus": 300,
    "field_keystroke": 3000,
    "sheet_drag": 2000,
    "field_keystroke_direct": 3000,
}
DUMPS = ["feed", "big", "feed_animated", "stress", "stress_animation", "grid", "cells"]


def build(checkout, target):
    """Builds the example in `checkout`, lent this checkout's source so that both sides have the
    same scenarios: an older copy of the example lacks the new ones. It must compile there."""
    example = checkout / "benchmarks/examples" / f"{EXAMPLE}.rs"
    theirs = example.read_bytes() if example.exists() else None
    if checkout != HERE.parent:
        example.parent.mkdir(exist_ok=True)
        shutil.copy(HERE / "examples" / f"{EXAMPLE}.rs", example)
    try:
        built = subprocess.run(
            ["cargo", "build", "--release", "-p", "compose-benchmarks", "--example", EXAMPLE],
            cwd=checkout,
            env={**os.environ, "CARGO_TARGET_DIR": str(target)},
        )
        if built.returncode != 0:
            lent = "This checkout's example, lent to it, doesn't compile there. " if (
                checkout != HERE.parent
            ) else ""
            sys.exit(f"{EXAMPLE} doesn't build in {checkout}. {lent}See the errors above.")
    finally:
        if checkout != HERE.parent:
            if theirs is None:
                example.unlink()
            else:
                example.write_bytes(theirs)
    return target / "release/examples" / EXAMPLE


def target_key(of):
    return hashlib.sha1(of.encode()).hexdigest()[:12]


def retired(binary, scenario, iterations):
    done = subprocess.run(
        ["/usr/bin/time", "-l", binary, scenario, str(iterations)], capture_output=True, text=True
    )
    if done.returncode != 0:
        sys.exit(f"{binary} {scenario}: exit {done.returncode}\n{done.stderr}")
    return int(re.search(r"(\d+)\s+instructions retired", done.stderr).group(1)) / iterations


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("base", help="a checkout's path, or a revision of this repository")
    parser.add_argument("scenarios", nargs="*", help="scenario:iterations (default: all)")
    parser.add_argument("--rounds", type=int, default=3, help="runs of each side, alternating")
    parser.add_argument("--dumps", action="store_true", help="compare display lists instead")
    parser.add_argument("--scratch", help="where the builds go (default: a temporary directory)")
    args = parser.parse_args()
    repo = HERE.parent
    scratch = pathlib.Path(args.scratch or tempfile.mkdtemp(prefix="compare-instructions-"))
    scratch.mkdir(parents=True, exist_ok=True)
    base = pathlib.Path(args.base)
    commit = None
    if not base.is_dir():
        commit = subprocess.run(
            ["git", "rev-parse", "--verify", args.base + "^{commit}"],
            cwd=repo,
            stdout=subprocess.PIPE,
            text=True,
            check=True,
        ).stdout.strip()
        source = scratch / f"base-src-{commit[:12]}"
        shutil.rmtree(source, ignore_errors=True)
        source.mkdir()
        archive = subprocess.run(
            ["git", "archive", commit], cwd=repo, stdout=subprocess.PIPE, check=True
        )
        # -m: the extraction's time, not the commit's. Cargo trusts a file older than its last
        # build, so a --scratch reused for a later revision would link the earlier one's crates.
        subprocess.run(["tar", "-x", "-m", "-C", source], input=archive.stdout, check=True)
        base = source
    base = base.resolve()
    # A target of its own for each, or another's binary stays: Cargo finds nothing newer than what it
    # built, the package's hash leaving out where the checkout is.
    binaries = {
        "base": build(base, scratch / f"base-target-{target_key(commit or str(base))}"),
        "this": build(repo, scratch / f"this-target-{target_key(str(repo))}"),
    }
    if args.dumps:
        same = True
        for screen in DUMPS:
            out = {}
            for side, binary in binaries.items():
                run = subprocess.run(
                    [binary, f"dump_{screen}", "30"], stdout=subprocess.PIPE, check=True
                )
                out[side] = run.stdout
            equal = out["base"] == out["this"]
            same &= equal
            print(f'{screen:20} {"same" if equal else "DIFFERENT":10} {len(out["this"])} bytes')
        print(f"Builds in {scratch}: delete them when done.")
        sys.exit(0 if same else 1)
    wanted = [
        (name, int(count)) for name, count in (one.split(":") for one in args.scenarios)
    ] or list(SCENARIOS.items())
    print(f'{"scenario":28} {"base":>12} {"this":>12} {"change":>8} {"spread":>8}  (per iteration)')
    for scenario, iterations in wanted:
        runs = {"base": [], "this": []}
        for round_ in range(args.rounds):
            for side in ("base", "this") if round_ % 2 == 0 else ("this", "base"):
                runs[side].append(retired(binaries[side], scenario, iterations))
        a, b = statistics.median(runs["base"]), statistics.median(runs["this"])
        spread = max(max(r) / min(r) - 1 for r in runs.values())
        print(
            f"{scenario:28} {a:12.0f} {b:12.0f} {100 * (b - a) / a:+7.2f}% {100 * spread:7.2f}%",
            flush=True,
        )
    print(f"Builds in {scratch}: delete them when done.")


if __name__ == "__main__":
    main()
