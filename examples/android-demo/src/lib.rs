//! The examples as an Android library, `libcompose_rs_demo.so`: `ComposeRsView(context,
//! "gallery")` in android/demo shows one.

#![allow(non_snake_case)]

use compose::foundation::layout::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

compose_android::export_apps! {
    "counter" => HostedCounter,
    "todo" => todo::TodoApp,
    "gallery" => gallery::Gallery,
}

/// The counter, with what the Kotlin app passes it (the tab it's in, a property) and an event
/// it sends back (shown by Kotlin in a toast): `ComposeRsView.setProp` and
/// `setOnEventListener`.
#[composable]
pub fn HostedCounter() {
    let props = LOCAL_HOST_PROPS.current();
    let events = LOCAL_HOST_EVENTS.current();
    let tab = props.string("tab");
    let send = move || {
        events.send("greeting", "Hello from Rust");
    };
    Column!({
        Text!(
            "In the Kotlin tab {}",
            tab.as_deref().unwrap_or("?"),
            modifier = Modifier.padding((16.dp(), 16.dp(), 16.dp(), 0.dp())),
        );
        TextButton!(on_click = send.clone(), modifier = Modifier.padding((8.dp(), 0.dp())), {
            Text!("Send an event to Kotlin");
        });
        counter::Counter();
    });
}
