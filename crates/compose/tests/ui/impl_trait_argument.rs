use compose::runtime::*;

#[composable]
fn Labelled(label: impl std::fmt::Display) {
    let _ = label.to_string();
}

fn main() {}
