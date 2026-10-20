use compose::runtime::*;

#[composable]
fn Editor(text: &mut String) {
    text.push('!');
}

fn main() {}
