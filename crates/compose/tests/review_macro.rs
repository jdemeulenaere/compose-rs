//! Regression tests from the adversarial review of phase B (`#[composable]`). Each test
//! demonstrated a finding, fixed since; its comment describes the defect.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use compose::runtime::*;

// ---- Test support (as in composables.rs) ---------------------------------------------------

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SERIAL: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn next_serial() -> u32 {
    SERIAL.with(|serial| serial.replace(serial.get() + 1))
}

fn new_composition() -> Composition {
    take_log();
    SERIAL.with(|serial| serial.set(0));
    Composition::new(Tree::new())
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("review::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// A row with its own remembered state: `id:t<serial>`.
#[composable]
fn Row(id: u32) {
    let serial = remember(next_serial);
    Leaf(&format!("{id}:t{serial}"));
}

// ---- 1. Group keys don't tell instantiations or same-named composables apart ---------------

trait Screen: 'static {
    const NAME: &'static str;
}
struct Home;
struct Settings;
impl Screen for Home {
    const NAME: &'static str = "home";
}
impl Screen for Settings {
    const NAME: &'static str = "settings";
}

#[composable]
fn Title<S: Screen>(size: u32) {
    log(format!("Title<{}>", S::NAME));
    Leaf(&format!("{} {size}", S::NAME));
}

/// Two instantiations of a generic composable have the same group key (crate + file + name)
/// and, here, the same kept type `(u32,)`. Called at the same position (through a table of
/// `fn` pointers, a plain Rust idiom), `Title::<Settings>` finds `Title::<Home>`'s group, sees
/// the same arguments and SKIPS: the UI keeps showing "home 1".
///
/// Expected: "settings 1" (a different function must not skip on another's group).
/// Actual: "home 1", and `Title<settings>` never runs.
#[test]
fn generic_instantiations_do_not_skip_on_each_others_group() {
    #[composable]
    fn Router(index: MutableState<usize>) {
        let screens: [fn(u32); 2] = [Title::<Home>, Title::<Settings>];
        screens[index.get()](1);
    }
    let composition = new_composition();
    let index = mutable_state_of(0usize);
    let handle = index.handle();
    composition.set_content(move || Router(handle));
    assert_eq!(tree(&composition), "home 1\n");
    take_log();
    index.set(1);
    composition.recompose();
    assert_eq!(tree(&composition), "settings 1\n", "log: {:?}", take_log());
}

mod light {
    use super::*;
    #[composable]
    pub fn Icon(size: u32) {
        Leaf(&format!("light {size}"));
    }
}

mod dark {
    use super::*;
    #[composable]
    pub fn Icon(size: u32) {
        Leaf(&format!("dark {size}"));
    }
}

/// Same-named composables in two inline modules of one file share a key (D47 accepts this as
/// "can only recreate or share state of the same type"). But with the same parameter types they
/// also share skipping: switching the theme leaves the light icon on screen.
///
/// Expected: "dark 24". Actual: "light 24" (dark::Icon skipped on light::Icon's group).
#[test]
fn same_named_composables_in_one_file_do_not_skip_on_each_others_group() {
    #[composable]
    fn Toolbar(dark_theme: MutableState<bool>) {
        let icon: fn(u32) = if dark_theme.get() { dark::Icon } else { light::Icon };
        icon(24);
    }
    let composition = new_composition();
    let dark_theme = mutable_state_of(false);
    let handle = dark_theme.handle();
    composition.set_content(move || Toolbar(handle));
    assert_eq!(tree(&composition), "light 24\n");
    dark_theme.set(true);
    composition.recompose();
    assert_eq!(tree(&composition), "dark 24\n");
}

#[composable]
fn KeyProbe() {}

/// D47 / docs/03: the key hashes "the source file (relative to the package)", "the same on
/// every machine". But cargo passes workspace members' sources to rustc relative to the
/// *workspace root* (`crates/compose/tests/review_macro.rs`), so `strip_prefix(CARGO_MANIFEST_DIR)`
/// never applies there and the key hashes the workspace-relative path; for a registry or git
/// dependency the path is absolute and gets stripped (`src/…`). The same composable thus has a
/// different key as a workspace member and as a dependency, after moving the package inside the
/// workspace, or under `--remap-path-prefix` / `trim-paths`. Harmless within one binary, but it
/// isn't what the design promises, and dumps containing keys aren't portable.
///
/// Expected: the key of `review_macro::tests/review_macro.rs::KeyProbe`. Actual: the key of
/// `review_macro::crates/compose/tests/review_macro.rs::KeyProbe`.
#[test]
fn group_keys_hash_the_path_relative_to_the_package() {
    let composition = new_composition();
    composition.set_content(KeyProbe);
    let expected = key_of("review_macro::tests/review_macro.rs::KeyProbe");
    let actual = key_of("review_macro::crates/compose/tests/review_macro.rs::KeyProbe");
    let dump = composition.dump();
    assert!(!dump.contains(&format!("{actual:#018x}")), "the workspace-relative key: {dump}");
    assert!(dump.contains(&format!("{expected:#018x}")), "{dump}");
}

// ---- 2. Per-statement groups break `key` in a loop with a `continue` -----------------------

/// In a loop with a `continue`, every statement with a likely composable call gets its own
/// region group, including the `key(…)` call. Each item's movable group then sits alone inside
/// its iteration's region instance (all with the same key, matched by position), so `key` only
/// ever searches one candidate: keyed identity is lost, and a reordered list recreates every
/// row's state instead of moving it. docs/03 says "`key` is outermost in the loop body"; with a
/// `continue` (the usual way to filter a list) it no longer is.
///
/// Expected: as without the `continue`, rows move with their state and skip: "3:t2\n1:t0\n2:t1\n".
/// Actual: every row is recreated ("3:t3\n1:t4\n2:t5\n").
#[test]
fn keyed_items_keep_their_state_in_a_loop_with_continue() {
    #[composable]
    fn Column(list: MutableState<Vec<u32>>) {
        for id in list.get() {
            if id == 0 {
                continue;
            }
            key(id, || Row(id));
        }
    }
    let composition = new_composition();
    let list = mutable_state_of(vec![1u32, 2, 3]);
    let handle = list.handle();
    composition.set_content(move || Column(handle));
    assert_eq!(tree(&composition), "1:t0\n2:t1\n3:t2\n");
    list.set(vec![3, 1, 2]);
    composition.recompose();
    assert_eq!(tree(&composition), "3:t2\n1:t0\n2:t1\n");
}

// ---- 3. Region groups that are missing --------------------------------------------------------

/// `let … else` is a branch: its `else` block runs instead of the rest of the body. The macro
/// gives `if`/`else` and `if let` branches region groups, but not the `else` of a `let … else`,
/// so the `Row` in the else block and the `Row` after it are the same positional call: state
/// jumps from the fallback row to the real one.
///
/// Expected (as with the equivalent `if let … else`, which gets regions): "5:t1", a new row.
/// Actual: "5:t0", the fallback row's remembered state.
#[test]
fn let_else_branch_gets_a_region_group() {
    #[composable]
    fn Holder(value: MutableState<Option<u32>>) {
        let Some(id) = value.get() else {
            Row(0);
            return;
        };
        Row(id);
    }
    let composition = new_composition();
    let value = mutable_state_of(None);
    let handle = value.handle();
    composition.set_content(move || Holder(handle));
    assert_eq!(tree(&composition), "0:t0\n");
    value.set(Some(5));
    composition.recompose();
    assert_eq!(tree(&composition), "5:t1\n");
}

/// The reference: the same logic with `continue`, which the macro handles with one group per
/// call statement, keeps each row's state with its call site.
#[composable]
fn WithContinue(skip: MutableState<u32>) {
    for i in 0..3 {
        Row(10 + i);
        if i == skip.get() {
            continue;
        }
        Row(20 + i);
    }
}

/// `break 'body` out of a labelled block in a loop body is a `continue` in all but name, but
/// only `continue` triggers the per-statement groups. Rows of the two call sites then share
/// positional groups, and state jumps between call sites.
///
/// Expected: the same tree as `WithContinue`. Actual: a row of the second call site takes the
/// state of a row of the first one.
#[test]
fn labelled_block_break_in_a_loop_is_treated_like_continue() {
    #[composable]
    fn WithLabelledBreak(skip: MutableState<u32>) {
        for i in 0..3 {
            'body: {
                Row(10 + i);
                if i == skip.get() {
                    break 'body;
                }
                Row(20 + i);
            }
        }
    }
    let run = |content: fn(MutableState<u32>)| {
        let composition = new_composition();
        let skip = mutable_state_of(1);
        let handle = skip.handle();
        composition.set_content(move || content(handle));
        skip.set(2);
        composition.recompose();
        tree(&composition)
    };
    let reference = run(WithContinue);
    assert_eq!(run(WithLabelledBreak), reference);
}

macro_rules! continue_if {
    ($condition:expr) => {
        if $condition {
            continue;
        }
    };
}

/// A `continue` inside a declarative macro (unlabelled `continue` isn't hygienic, so this is
/// ordinary Rust) isn't seen by the macro's `continue` scan: no per-statement groups.
///
/// Expected: the same tree as `WithContinue`. Actual: state jumps between call sites.
#[test]
fn continue_inside_a_macro_is_treated_like_continue() {
    #[composable]
    fn WithMacroContinue(skip: MutableState<u32>) {
        for i in 0..3 {
            Row(10 + i);
            continue_if!(i == skip.get());
            Row(20 + i);
        }
    }
    let run = |content: fn(MutableState<u32>)| {
        let composition = new_composition();
        let skip = mutable_state_of(1);
        let handle = skip.handle();
        composition.set_content(move || content(handle));
        skip.set(2);
        composition.recompose();
        tree(&composition)
    };
    let reference = run(WithContinue);
    assert_eq!(run(WithMacroContinue), reference);
}

// ---- 4. Callbacks taking references make a composable non-restartable ------------------------

/// `Rc<dyn Fn(&str)>` is `'static` and a `ComposeArg` (compared by identity), but the macro's
/// type scan sees the `&` inside the `Fn` sugar and makes the composable non-restartable: it
/// never skips, so every recomposition of its parent re-runs it (and its whole subtree). This is
/// the most common callback shape (`on_change`, `on_text`), so most input composables lose
/// skipping. Same for `fn(&str)` and `for<'a> …` types.
///
/// Expected: the child skips (identical `Rc`). Actual: it runs on every parent recomposition.
#[test]
fn a_callback_taking_a_reference_keeps_a_composable_restartable() {
    #[composable]
    fn Field(value: u32, on_change: Rc<dyn Fn(&str)>) {
        log(format!("Field {value}"));
        on_change("x");
    }
    #[composable]
    fn Form(trigger: MutableState<i32>, on_change: Rc<dyn Fn(&str)>) {
        log(format!("Form {}", trigger.get()));
        Field(1, on_change.clone());
    }
    let composition = new_composition();
    let trigger = mutable_state_of(0);
    let handle = trigger.handle();
    let on_change: Rc<dyn Fn(&str)> = Rc::new(|_| {});
    composition.set_content(move || Form(handle, on_change.clone()));
    take_log();
    trigger.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["Form 1"], "Field should skip");
}

// ---- 5. A panic caught inside a composable leaves its groups open ----------------------------

#[composable]
fn Fragile(fail: bool) {
    Leaf("fragile");
    if fail {
        panic!("caught by the caller");
    }
}

/// Group guards don't end their groups while a panic unwinds (D41: "the pass is poisoned
/// anyway"). But a panic caught inside the composition by the user's `catch_unwind` doesn't
/// poison the pass: the body goes on with `Fragile`'s group (and its region) still open, so
/// `Leaf("after")` is composed *inside* `Fragile`'s group, the caller's guard ends the wrong
/// group, and the pass fails at its end with an "unclosed groups" misuse that points nowhere
/// near the cause. Nothing rejects or diagnoses a `catch_unwind` around a composable call
/// (Compose forbids try/catch around composable calls at compile time).
///
/// Expected: either the groups stay balanced, or a diagnostic that names the caught panic. The
/// runtime gives the diagnostic (D51): ending groups while a panic unwinds could panic again and
/// abort, and Compose itself forbids catching around composable calls.
#[test]
fn a_panic_caught_inside_a_composable_is_diagnosed() {
    #[composable]
    fn Guarded() {
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Fragile(true))).is_err();
        assert!(caught);
        Leaf("after");
    }
    let composition = new_composition();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| composition.set_content(Guarded)));
    let message = result.err().map(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_default()
    });
    assert!(message.is_some_and(|message| message.contains("was caught inside the composition")));
    assert!(composition.is_poisoned());
}
