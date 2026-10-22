use compose::runtime::*;

#[derive(Clone, ComposeArg)]
struct Model {
    #[compose_arg(deep)]
    items: Vec<String>,
}

fn main() {}
