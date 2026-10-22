// A vertical arrangement given to a `Row` (Compose's `Arrangement.Top` is an
// `Arrangement.Vertical`, which `Row(horizontalArrangement = …)` doesn't take).
use compose::foundation::layout::*;
use compose::runtime::*;

#[composable]
fn Bar() {
    Row!(horizontal_arrangement = Arrangement::Top, {});
}

fn main() {}
