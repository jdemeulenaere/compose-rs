//! Regression tests from the third review of phase B (`#[composable]`, after 10a535b). Each
//! test fails at 10a535b; its comment describes the defect.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::fmt;

use compose::runtime::platform::*;
use compose::runtime::*;

// ---- Test support (as in review_macro2.rs) -------------------------------------------------

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

fn next_serial() -> u32 {
    SERIAL.with(|serial| serial.replace(serial.get() + 1))
}

fn new_composition() -> Composition {
    LOG.with(|log| log.borrow_mut().clear());
    SERIAL.with(|serial| serial.set(0));
    Composition::new(Tree::new())
}

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("review3::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// Composes `content` with `flag` false, then true, and returns both trees.
fn run_flag(content: fn(MutableState<bool>)) -> (String, String) {
    let composition = new_composition();
    let flag = mutable_state_of(false);
    let handle = flag.handle();
    composition.set_content(move || content(handle));
    let first = tree(&composition);
    flag.set(true);
    composition.recompose();
    (first, tree(&composition))
}

// ---- 1. Colliding keys still match for non-restartable composables -------------------------

mod light {
    use super::*;
    /// Non-restartable: it returns a value.
    #[composable]
    pub fn Accent(level: u32) -> String {
        let serial = remember(next_serial);
        format!("light {level}:t{serial}")
    }
}

mod dark {
    use super::*;
    #[composable]
    pub fn Accent(level: u32) -> String {
        let serial = remember(next_serial);
        format!("dark {level}:t{serial}")
    }
}

/// docs/03 ("Colliding keys never match") and D47 say composables whose keys collide (two
/// instantiations of a generic composable, same-named composables in one file's inline
/// modules) never match each other's groups, so neither gets the other's state. 10a535b
/// implements it only for restart groups, by comparing restart functions. A non-restartable
/// composable (one returning a value, taking a borrow, or `#[composable(non_restartable)]`)
/// starts a plain replace group keyed by crate + file + name: `dark::Accent` reuses the group
/// of `light::Accent` and reads its remembered state, which belongs to another function.
///
/// Expected: "dark 1:t1\n" (dark::Accent's own state). Actual: "dark 1:t0\n".
#[test]
fn same_named_non_restartable_composables_do_not_share_remembered_state() {
    #[composable]
    fn Toolbar(dark_theme: MutableState<bool>) {
        let accent: fn(u32) -> String = if dark_theme.get() { dark::Accent } else { light::Accent };
        Leaf(&accent(1));
    }
    let (first, second) = run_flag(Toolbar);
    assert_eq!(first, "light 1:t0\n");
    assert_eq!(second, "dark 1:t1\n");
}

trait Theme: 'static {
    const NAME: &'static str;
}
struct Light;
struct Dark;
impl Theme for Light {
    const NAME: &'static str = "light";
}
impl Theme for Dark {
    const NAME: &'static str = "dark";
}

#[composable]
fn Swatch<T: Theme>(level: u32) -> String {
    let serial = remember(next_serial);
    format!("{} {level}:t{serial}", T::NAME)
}

/// As above, for two instantiations of a generic non-restartable composable: `Swatch::<Dark>`
/// runs in `Swatch::<Light>`'s group and takes its remembered state.
///
/// Expected: "dark 1:t1\n". Actual: "dark 1:t0\n".
#[test]
fn generic_non_restartable_instantiations_do_not_share_remembered_state() {
    #[composable]
    fn Toolbar(dark_theme: MutableState<bool>) {
        let swatch: fn(u32) -> String =
            if dark_theme.get() { Swatch::<Dark> } else { Swatch::<Light> };
        Leaf(&swatch(1));
    }
    let (first, second) = run_flag(Toolbar);
    assert_eq!(first, "light 1:t0\n");
    assert_eq!(second, "dark 1:t1\n");
}
