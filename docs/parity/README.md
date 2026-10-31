# Parity with Compose

compose-rs aims at 1:1 parity with state-of-the-art Jetpack Compose: every public, non-deprecated API, the `@Experimental*` ones included, with Compose's behaviour, names and defaults, Rust-shaped; and Compose's own tests adapted as Rust tests (D700). This folder is the plan and the record of that work.

## The reference

The androidx checkout at `/Users/jdemeulenaere/Development/androidx-main` (Compose under `frameworks/support/compose`, Navigation 3 under `frameworks/support/navigation3`), at its tip: the ledgers were made at commit `d153d87ed76`. Agents read it, read-only: never modify it, never run its build or Gradle, never run anything in it that downloads. Where the tip and the Compose 1.12.1 that the Android benchmarks ran differ, the tip wins, parity mode included (D705).

**In scope:** `compose/runtime`, `compose/ui` (ui, ui-text, ui-graphics, ui-geometry, ui-unit, ui-util, ui-test), `compose/foundation` (foundation, foundation-layout), `compose/animation` (animation-core, animation, animation-graphics), `compose/material3` (material3-ripple inside it; the adaptive libraries, window size class and navigation suite last), Navigation 3 with a lifecycle and ViewModel model (D704) and the navigation events under it (`navigationevent`, activity-compose's back handlers). **Out of scope:** Material 2, Wear, TV, Glance, remote, Navigation Compose (replaced by Navigation 3), material3-a2ui.

## The ledgers

[What's missing](missing.md) is a short overview of the big gaps. The ledgers have the detail.

One per area. Each lists every Compose test file with its status, the public API compose-rs lacks, the missing features with their dependencies and sizes, and notes for porting:

| Ledger | Area |
|---|---|
| [runtime](runtime.md) | `compose/runtime/*` |
| [ui-core](ui-core.md) | ui layout, lookahead, nodes, modifiers, drawing, graphics, geometry, unit, util, popups |
| [ui-input](ui-input.md) | pointer and key input, nested scroll, focus, semantics, accessibility, hosts |
| [ui-text-and-test](ui-text-and-test.md) | ui-text; ui-test, the test framework compose-rs's harness must match |
| [foundation-layout-lazy](foundation-layout-lazy.md) | foundation-layout, gestures, scrolling, lazy layouts, pager |
| [foundation-text-misc](foundation-text-misc.md) | text, text fields, selection, clickable, interactions, images, shapes and the rest of foundation |
| [animation-navigation](animation-navigation.md) | animation-core, animation, animation-graphics, Navigation 3, navigationevent |
| [lifecycle](lifecycle.md) | androidx.lifecycle: lifecycles and ViewModels (D704) |
| [material3](material3.md) | Material 3 |

Statuses: `covered` (compose-rs tests exercise it, cited), `port` (the feature exists, tests to adapt), `partial`, `feature:<name>` (waits on a missing feature), `n/a: <reason>`, and, once done, `ported` (with the compose-rs test files).

## How the work is done

- **Tests are rewritten, not copied.** A Rust test is written from the Compose test's scenario and expected behaviour, with a name of its own in full words, and cites the test it adapts on the line above it: `// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/lazy/list/LazyListTest.kt, itemsAreNotRecomposedWhenScrolled`. One Compose test can give several Rust tests, or several Compose tests one. No Compose code, comments or license headers are copied, and a script translating Kotlin tests into Rust counts as copying, whatever it leaves out.
- **A ported test that fails is a bug to fix,** in compose-rs, not a test to bend: the test keeps Compose's expected behaviour. Where compose-rs deliberately differs (a platform with no analogue), the ledger says so and why, and a decision row records it.
- **Missing features** are implemented with Compose's names, defaults and behaviour, test first, in the dependency order the ledgers give.
- **Ledgers stay current:** a piece of work updates its rows' statuses and its features' entries in the same commit.
- **Decisions** made along the way are rows in the decision log as usual, "(agent)" for an agent's. Each workstream has its own block of numbers, given in its brief, so parallel branches don't collide.
- **Performance:** compose-rs's speed is a feature. A change on a hot path (composition, layout, drawing, lazy layouts, text) runs the relevant headless benchmarks (`benchmarks/benches`) before and after and reports the change; a regression needs a reason, recorded. Timing swings up to 4x on a loaded machine, so `benchmarks/compare_instructions.py <revision>` counts the instructions each side retires on fixed work (ui, lazy and animation scenarios), by median: what to quote when the two disagree (docs/14, "Method").
- **Compile speed:** tests go into each crate's existing `tests/suite` binary (one per crate), not new test binaries.
- **Reviews:** every merged piece has had an adversarial review by a fresh agent.
