use compose::runtime::*;

#[composable]
fn Label(text: String, #[default(1)] size: u32) {
    let _ = (text, size);
}

#[composable]
fn Screen() {
    Label!("x".into(), colour = 3);
}

fn main() {}
