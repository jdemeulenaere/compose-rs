// Phase C review: `impl Fn(&str) + 'static` is unsupported (D59), but the definition compiles;
// every call then fails with "implementation of `IntoCallback` is not general enough", pointing
// at the callee's `#[composable]` attribute. The definition should be rejected with a message
// saying to take the argument by value (`impl Fn(String)`).
use compose::runtime::*;

#[composable]
fn Field(on_change: impl Fn(&str) + 'static) {
    on_change("x");
}

fn main() {}
