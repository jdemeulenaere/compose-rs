use compose::runtime::*;

#[composable]
fn Bar(title: String) {
    let _ = title;
}

#[composable]
fn Frame(#[default(|| {})] top_bar: impl Fn() + 'static, content: impl Fn() + 'static) {
    top_bar();
    content();
}

#[composable]
fn Screen() {
    Frame!(top_bar = Bar!("Title".into()), {});
}

fn main() {}
