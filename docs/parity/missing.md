# What's missing for parity with Compose

A short overview, written on 2026-10-31. The [ledgers](README.md) have the detail. Numbers are the Compose tests the ledgers count as waiting on a feature, tallied roughly: they show size, not exact remaining work, and some ledger tags lag behind what's done.

Out of scope, by decision: Material 2, Wear, TV, Glance, remote, Navigation Compose. iOS and WASM are paused.

## Written, not on main yet

These are on local branches, so they're not lost, but each still has to finish its review or fixes:

| Work | Branch | State |
|---|---|---|
| Lookahead: `LookaheadScope`, `approach_layout`, intrinsics in a scope | `worktree-agent-a6e1d41d0641ba30a` | Reviewed and fixed, waiting for Miri. Lazy items' lookahead positions and most of `LookaheadScopeTest` are left. |
| `SubcomposeLayout`, `BoxWithConstraints` on it | `worktree-agent-a12dfd51d71063a72` | Review found a blocker: reused content panics. Being fixed. Paused precomposition is left. |
| `animate_item` for lazy layouts | `worktree-agent-ae41dc220348e3a1a` | In review. Grid and staggered tests are only partly ported. |
| Stretch and glow overscroll | `worktree-agent-ad25708f190fdec81` | Being built, not reviewed. Design: [stretch-overscroll.md](design/stretch-overscroll.md). |

## The big missing things

### 1. Modern text fields (the largest gap)
- **`TextFieldState` and the state-based `BasicTextField`** (about 700 tests), with `InputTransformation`, `OutputTransformation`, `TextFieldLineLimits`, undo, and `BasicSecureTextField` (about 100). Today's fields are value-based. The keyboard path it needed is now on main.
- **Touch text selection** (about 230): handles and selection gestures in text and fields.
- **Text context menu** (about 290), **magnifier** (about 80), **handwriting** (about 85), **autofill** (about 165), **text links** (about 55).

### 2. Layout and composition foundations
- **After `SubcomposeLayout`:** lazy layouts moved onto subcomposition (C8), so items compose during measure as in Compose. Today they compose in rounds, the root of several differences.
- **Shared elements and `animateBounds`** (about 95), which wait on lookahead.
- **`Modifier.onLayoutRectChanged` and its callbacks** (about 160).
- **`FlexBox`** (about 95) and **the grid layout** (about 80).
- **Android view interop**, `AndroidView` and hosting (about 280).
- **Runtime:** computed state (about 55), composition observers, error traces, composition data.

### 3. Graphics
- **`GraphicsLayer` as an object** (about 95), **blend modes and color filters** (about 85), **color spaces** (about 70), **`Painter`** (about 45), offscreen canvas and bitmaps.
- **Vector graphics** (about 90) and **animated image vectors** (about 20).
- **Blur** (about 80), **drop and inner shadows** (about 45), brush and path extras, polygon shapes (about 65).

### 4. Foundation
- **Style API** (about 160), **border parity** (about 80).
- **Gestures and scrolling:** `transformable` (about 40), 2D scrolling (about 45), `bringIntoView` (about 90 across two rows), public lazy layout API (about 40), `BeyondBoundsLayout` (about 35).

### 5. Material 3
- **Whole components missing:** carousel (about 100), floating toolbar (about 80), search bar (about 65), toggle button (about 50), wide navigation rail (about 45), button group (about 45), wavy progress (about 45), short navigation bar (about 35), split button (about 35), loading indicator (about 20), FAB menu (about 15).
- **Dynamic color** (about 25), `TimeScroll` and time picker dialogs, Material transitions.
- **Adaptive libraries,** window size classes and the navigation suite, planned last.
- Many existing components are `partial`: they exist, with tests or details left.

### 6. Input and platform
- **Indirect pointer input** (about 125), **pointer icons** (about 60), scroll capture (about 45), clipboard API (about 30), content capture, drag and drop, rotary input, media queries.
- **Locale and device data:** the date and time pickers use small tables for a few languages. Real data needs `icu4x`, a decision for the user.

### 7. The test framework
`compose-test` lacks parts of ui-test that ports will need: pointer, trackpad and indirect injection (about 175 tests of its own), device configuration override, and capture to image.

## Not verified on a device

Written and tested on the host, but not yet confirmed on a phone:

- **TalkBack's speech and announcements.** The accessibility tree was read directly, but no agent can enable a service.
- **A keyboard that really composes.** Gboard on the test phone doesn't compose, so long-lived composition hasn't run on a real keyboard (D1671).
- A real D-pad and a real mouse, rotation, voice typing.
- Android's real `RippleDrawable` (`feature:android-platform-ripple`).

## Known costs against Compose's shape

- A focus change in a text field costs about 10 % more than before the decoration box work, and a keystroke about 5 %.
- Movable content moved across compositions re-runs its body, and doesn't move between different trees, such as into a `Dialog` (D1308).
