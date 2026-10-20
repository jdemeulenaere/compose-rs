use compose::runtime::*;

#[derive(Clone, ComposeArg)]
struct Model {
    id: u64,
    items: Vec<String>,
}

fn main() {}
