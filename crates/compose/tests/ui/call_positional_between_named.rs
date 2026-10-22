use compose::runtime::*;

#[composable]
fn Button(
    on_click: impl Fn() + 'static,
    #[default(true)] enabled: bool,
    content: impl Fn() + 'static,
) {
    let _ = (on_click, enabled, content);
}

#[composable]
fn Screen() {
    Button!(on_click = || {}, { println!("first") }, { println!("second") });
}

fn main() {}
