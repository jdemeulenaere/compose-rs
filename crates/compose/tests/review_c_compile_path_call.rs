//! Phase C review: valid code that fails to compile.
//!
//! A call macro expands to a bare `Name(…)`, resolved where the macro is used, so calling it by
//! path (`ui::Label!()`, as one would call `ui::Label()`) works only if the function is also
//! imported there: E0425 "cannot find function `Label` in this scope". With another `Label` in
//! scope it silently calls that one (`review_c::a_path_call_macro_calls_the_function_it_names`).
//!
//! Expected: compiles. Actual: E0425.

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
fn Screen(n: u32) {
    ui::Label!("count {n}");
}

#[test]
fn compiles() {}
