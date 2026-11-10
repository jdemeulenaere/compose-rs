//! Adversarial review tests for `perform_scroll_to_key` (`D2337`): non-`Debug` custom key types,
//! `String` / `&'static str` cross-lookup, lazy grids, staggered grids, pagers, and selector errors.

use std::hash::Hash;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, Modifier, UnitExt};

#[derive(Clone, PartialEq, Eq, Hash)]
struct OpaqueKey(u32);

#[track_caller]
fn expect_panic(action: impl FnOnce()) -> String {
    let err =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)).expect_err("expected panic");
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_default()
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    compose_test::use_optimizations(Optimizations::NONE);
    ComposeTestRule::sized(IntSize::new(400, 400), content)
}

#[test]
fn perform_scroll_to_key_supports_opaque_keys_string_interop_grids_staggered_and_pagers() {
    let mut ui = rule(|| {
        Column!(|_| {
            LazyColumn!(
                modifier = Modifier.required_size(90.dp()).test_tag("opaque_list"),
                |scope: LazyListScope| {
                    scope.items_keyed(
                        15,
                        |i| OpaqueKey(i as u32),
                        |i| {
                            Spacer(Modifier.required_size(30.dp()).test_tag(format!("ol_{i}")));
                        },
                    );
                }
            );
            LazyVerticalGrid!(
                GridCells::Fixed(2),
                modifier = Modifier.required_size(90.dp()).test_tag("grid"),
                |scope: LazyGridScope| {
                    scope.items_keyed(
                        20,
                        |i| format!("gk_{i}"),
                        |i| {
                            Spacer(Modifier.required_size(30.dp()).test_tag(format!("gi_{i}")));
                        },
                    );
                }
            );
            LazyVerticalStaggeredGrid!(
                StaggeredGridCells::Fixed(2),
                modifier = Modifier.required_size(90.dp()).test_tag("staggered"),
                |scope: LazyStaggeredGridScope| {
                    scope.items_keyed(
                        20,
                        |i| "sk_0".to_string() + &i.to_string(),
                        |i| {
                            Spacer(Modifier.required_size(30.dp()).test_tag(format!("si_{i}")));
                        },
                    );
                }
            );
            HorizontalPager!(
                remember_pager_state!(0, 0.0, || 10),
                modifier = Modifier.required_size(90.dp()).test_tag("pager"),
                key = Some(pager_key(|p| OpaqueKey(100 + p as u32))),
                page_content = |_, p| {
                    Spacer(Modifier.fill_max_size().test_tag(format!("pg_{p}")));
                }
            );
        });
    });

    // Opaque non-Debug key scrolls LazyColumn:
    ui.on_node_with_tag("ol_12").assert_does_not_exist();
    ui.on_node_with_tag("opaque_list").perform_scroll_to_key(OpaqueKey(12));
    ui.on_node_with_tag("ol_12").assert_is_displayed();

    // Grid & StaggeredGrid String <-> &'static str interop:
    ui.on_node_with_tag("grid").perform_scroll_to_key("gk_14");
    ui.on_node_with_tag("gi_14").assert_is_displayed();

    ui.on_node_with_tag("staggered").perform_scroll_to_key("sk_015");
    ui.on_node_with_tag("si_15").assert_is_displayed();

    // Pager with OpaqueKey:
    ui.on_node_with_tag("pager").perform_scroll_to_key(OpaqueKey(107));
    ui.on_node_with_tag("pg_7").assert_is_displayed();

    // Multiple nodes matching has_scroll_to_key_action() reports Compose's selector failure prefix:
    let multi_msg = expect_panic(|| {
        ui.on_node(has_scroll_to_key_action()).perform_scroll_to_key("gk_0");
    });
    assert!(multi_msg.contains("Failed: performScrollToKey(\"gk_0\")"), "{multi_msg}");
}

#[test]
fn perform_scroll_to_key_missing_opaque_key_names_concrete_key_type() {
    let mut ui = rule(|| {
        LazyColumn!(
            modifier = Modifier.required_size(90.dp()).test_tag("opaque_list"),
            |scope: LazyListScope| {
                scope.items_keyed(
                    5,
                    |i| OpaqueKey(i as u32),
                    |i| {
                        Spacer(Modifier.required_size(30.dp()).test_tag(format!("ol_{i}")));
                    },
                );
            }
        );
    });
    let missing_msg = expect_panic(|| {
        ui.on_node_with_tag("opaque_list").perform_scroll_to_key(OpaqueKey(999));
    });
    assert!(missing_msg.contains("OpaqueKey"), "{missing_msg}");
    assert!(missing_msg.contains("couldn't find the key."), "{missing_msg}");
}
