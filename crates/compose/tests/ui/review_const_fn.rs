// Review: a `const fn` composable isn't rejected; rustc then reports seven E0015/E0493 errors
// on the `#[composable]` attribute (current(), RestartGroup::start, update_kept, skipping,
// skip_to_group_end, the guard's destructor, ...) instead of one error on `const`.
use compose::runtime::*;

#[composable]
const fn Badge(count: u32) {}

fn main() {}
