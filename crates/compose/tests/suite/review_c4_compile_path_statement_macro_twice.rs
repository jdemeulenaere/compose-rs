//! Phase C fourth review: valid code that `#[composable]` fails to compile.
//!
//! The third review's fix imports a path statement macro's path as a statement in the
//! enclosing block (`ui::Label!("a");` becomes `use ui::Label; Label!("a");`). Calling the same
//! composable by path twice in one block, the most ordinary use of a path call, then imports
//! `Label` twice in one scope.
//!
//! Expected: compiles. Actual: E0252 "the name `Label` is defined multiple times".

#![allow(non_snake_case, dead_code)]

use compose::runtime::*;

mod ui {
    use compose::runtime::*;

    #[composable]
    pub fn Label(#[format] text: String) {
        let _ = text;
    }
}

#[composable]
fn Screen() {
    ui::Label!("first");
    ui::Label!("second");
}

#[test]
fn compiles() {}
