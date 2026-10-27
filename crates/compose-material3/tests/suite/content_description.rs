//! Content descriptions built at run time, as Compose's `contentDescription: String?` (finding
//! 14, D1022): `Icon` and `Scrim` take `Some` of a literal, a `String` or a `SharedString`,
//! `None`, or an `Option` of any of them.

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{Modifier, SharedString, UnitExt};

#[test]
fn an_icons_description_can_be_built_at_run_time() {
    let mut ui = compose_test(|| {
        let count = 3;
        Icon!(icons::filled::email(), Some(format!("{count} new messages")));
        let owned: String = "Owned".to_string();
        Icon!(icons::filled::edit(), Some(owned));
        let shared: Option<SharedString> = Some("Shared".into());
        Icon!(icons::filled::lock(), shared);
        let maybe: Option<&'static str> = None;
        Icon!(icons::filled::settings(), maybe);
        Icon!(icons::filled::star(), Some("Literal"));
        Icon!(icons::filled::check(), None);
    });
    for description in ["3 new messages", "Owned", "Shared", "Literal"] {
        ui.on_node_with_content_description(description).assert_exists();
    }
}

#[test]
fn a_scrims_description_can_be_built_at_run_time() {
    let mut ui = compose_test(|| {
        let what = "sheet";
        Box!(modifier = Modifier.size(100.dp()), {
            Scrim!(Some(format!("Close {what}")));
        });
    });
    ui.on_node_with_content_description("Close sheet").assert_exists();
}
