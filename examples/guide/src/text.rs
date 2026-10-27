//! Text, text fields, the keyboard and focus (docs/guide/kotlin-to-rust.md, "Text and text
//! fields").

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: text
#[composable]
pub fn Headline(title: SharedString, summary: SharedString) {
    Column!({
        Text!(title, style = MaterialTheme::typography().headline_small.clone(), max_lines = 1);
        Text!(
            summary,
            color = MaterialTheme::color_scheme().on_surface_variant,
            max_lines = 3,
            overflow = TextOverflow::Ellipsis,
        );
        // Format arguments, as `format!`: inline names too.
        let count = 3;
        Text!("{count} unread", font_weight = Some(FontWeight::BOLD));
    });
}
// ANCHOR_END: text

// ANCHOR: annotated
#[composable]
pub fn Terms() {
    // `buildAnnotatedString { append(…); withStyle(…) { append(…) } }`
    let text = AnnotatedString::build(|text| {
        text.append("By joining, you accept the ");
        let bold = SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::default() };
        text.with_style(bold, |text| {
            text.append("rules");
        });
        text.append(".");
    });
    AnnotatedText!(text); // Material's `Text(AnnotatedString)`
}
// ANCHOR_END: annotated

// ANCHOR: fields
#[composable]
pub fn JoinNetwork(on_join: impl Fn(String, String) + 'static) {
    let network = remember_saveable(|| mutable_state_of(String::new()));
    let passphrase = remember(|| mutable_state_of(String::new()));
    let network_field = remember(FocusRequester::new);
    let focus_manager = LOCAL_FOCUS_MANAGER.current();
    let join = callback(move || {
        focus_manager.clear_focus(false); // hides the soft keyboard with the focus
        on_join(network.get(), passphrase.get());
    });
    Column!(
        modifier = Modifier.padding(16.dp()),
        vertical_arrangement = Arrangement::spaced_by(8.dp()),
        {
            OutlinedTextField!(
                network.get(),
                move |value| network.set(value),
                modifier = Modifier.focus_requester(network_field).test_tag("network"),
                label = Some(|| Text!("Network")),
                single_line = true,
                // `Next`'s default action moves the focus to the next field.
                keyboard_options =
                    KeyboardOptions { ime_action: ImeAction::Next, ..KeyboardOptions::DEFAULT },
            );
            OutlinedTextField!(
                passphrase.get(),
                move |value| passphrase.set(value),
                modifier = Modifier.test_tag("passphrase"),
                label = Some(|| Text!("Passphrase")),
                single_line = true,
                visual_transformation = PasswordVisualTransformation::new(),
                keyboard_options = KeyboardOptions {
                    keyboard_type: KeyboardType::Password,
                    ime_action: ImeAction::Done,
                    ..KeyboardOptions::DEFAULT
                },
                keyboard_actions = KeyboardActions::new().on_done(move || join()),
            );
            Button!(on_click = join, enabled = !network.get().is_empty(), {
                Text!("Join");
            });
        }
    );
    // Focus the first field once shown, as `LaunchedEffect(Unit) { requester.requestFocus() }`.
    LaunchedEffect((), async move {
        network_field.request_focus();
    });
}
// ANCHOR_END: fields
