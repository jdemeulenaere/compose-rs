use compose::runtime::*;

#[composable]
fn Card(
    #[default(None)] on_click: Option<impl Fn() + 'static>,
    #[default(true)] enabled: bool,
    content: impl Fn() + 'static,
) {
    let _ = (on_click, enabled, content);
}

fn handle_click() {}

/// After named arguments, only a block or a closure is the trailing lambda: a value there is
/// more likely a misplaced argument than the content.
#[composable]
fn Screen() {
    Card!(enabled = false, handle_click);
}

fn main() {}
