use compose::runtime::*;

struct Handle(u32);

impl ComposeArg for Handle {
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[composable]
fn Show(handle: Handle) {
    let _ = handle.0;
}

fn main() {}
