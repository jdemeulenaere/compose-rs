use compose::runtime::*;

#[composable]
fn Label(text: String, #[default(1)] size: u32) {
    let _ = (text, size);
}

#[composable]
fn Screen() {
    Label!(size = 2);
}

fn main() {}
