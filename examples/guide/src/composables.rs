//! Composable functions and their calls (docs/guide/kotlin-to-rust.md, "Composables").

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: greeting
/// Greets `name`: a documented composable's rustdoc shows its call form and defaults.
#[composable]
pub fn Greeting(
    name: impl Into<SharedString>,
    #[default(Modifier)] modifier: Modifier,
    #[default(false)] excited: bool,
) {
    let mark = if excited { "!" } else { "." };
    Text!("Hello, {}{}", name, mark, modifier = modifier);
}
// ANCHOR_END: greeting

// ANCHOR: calls
#[composable]
pub fn Greetings() {
    Column!({
        Greeting!("Ada"); // the defaults for the rest
        Greeting!("Grace", excited = true); // positional, then named
        Greeting!(name = "Linus", modifier = Modifier.padding(8.dp()));
        // The function itself, every argument given: what the call macro expands to.
        Greeting(SharedString::from("Barbara"), Modifier, false);
    });
}
// ANCHOR_END: calls

// ANCHOR: content
/// A titled section: `content` is a composable lambda, as Kotlin's `content: @Composable () -> Unit`.
#[composable]
pub fn Section(title: impl Into<SharedString>, content: impl Fn() + 'static) {
    Column!(modifier = Modifier.padding(8.dp()), {
        Text!(title, style = MaterialTheme::typography().title_medium.clone());
        content();
    });
}

#[composable]
pub fn Fruit() {
    // A final block is the trailing lambda.
    Section!("Fruit", {
        Text!("Apple");
        Text!("Pear");
    });
    // A slot given by name takes a block too.
    Section!(
        title = "Vegetables",
        content = {
            Text!("Leek");
        }
    );
}
// ANCHOR_END: content
