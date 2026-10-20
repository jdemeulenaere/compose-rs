//! Compile-fail tests: every construct `#[composable]` rejects, and the missing-`ComposeArg`
//! diagnostic. Regenerate the expected output with `TRYBUILD=overwrite cargo test -p compose`.

#[test]
fn rejected_constructs() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
