use compose::runtime::*;

#[composable]
fn Button(on_click: impl Fn() + 'static, content: impl Fn() + 'static) {
    on_click();
    content();
}

/// The `on_click` is forgotten: the block is taken for it, and the error names `content`.
#[composable]
fn Screen() {
    Button!({});
}

fn main() {}
