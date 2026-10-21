use compose::runtime::*;

#[composable]
fn Label(text: String, #[default(1)] size: u32) {
    let _ = (text, size);
}

#[composable]
fn Screen() {
    Label!("x".into(), size = 2, size = 3);
}

fn main() {}
