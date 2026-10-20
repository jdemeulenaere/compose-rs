// An associated function without a receiver can't be a composable (D45): the restart function
// calls it unqualified, and the macro can't see the `impl` block. rustc then reports "cannot
// find function `Content`" at its name. This pins that message; a clearer one needs the macro to
// know it is inside an `impl`, which a proc macro on a function can't.
use compose::runtime::*;

struct Screen;

impl Screen {
    #[composable]
    fn Content(id: u32) {}
}

fn main() {}
