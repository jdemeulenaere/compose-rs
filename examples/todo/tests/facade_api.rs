//! An app written against the facade's top level only (`compose::*` without `platform`), with
//! the examples' patterns: a custom layout, a custom modifier node, custom drawing, semantics,
//! a custom arrangement, and the D263/D264 call forms. It compiles only while every type these
//! need is nameable from the app-facing surface (D260, D265).

#![allow(non_snake_case)]

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::*;

// ---- A custom modifier node: draws behind, adds semantics, pads by layout ---------------------

#[derive(PartialEq)]
struct Marker {
    color: Color,
}

struct MarkerNode {
    color: Color,
}

impl ModifierNodeElement for Marker {
    type Node = MarkerNode;
    fn create(&self) -> MarkerNode {
        MarkerNode { color: self.color }
    }
    fn update(&self, node: &mut MarkerNode) {
        node.color = self.color;
    }
    fn same(&self, other: &Self) -> bool {
        other == self
    }
}

impl ModifierNode for MarkerNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for MarkerNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let mut paint = Paint::stroke(self.color, 2.0);
        paint.cap = StrokeCap::Round; // fields stay writable
        scope.draw_rect(Rect::new(0.0, 0.0, size.width, size.height), paint);
        scope.draw_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(self.color).with_alpha(0.5));
    }
}

impl SemanticsModifierNode for MarkerNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.content_description.push("marker".into());
        config.role = Some(Role::Image);
    }
}

impl LayoutModifierNode for MarkerNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

fn marker(modifier: Modifier, color: Color) -> Modifier {
    modifier.then_custom(Marker { color })
}

/// Names the scopes custom drawing and gestures take.
fn _scopes(
    _: &mut ContentDrawScope<'_, '_>,
    _: &CacheDrawScope,
    _: Option<DrawResult>,
    _: Option<AwaitPointerEvent>,
) {
}

// ---- A custom arrangement --------------------------------------------------------------------

struct Reversed;

impl HorizontalArrangementPolicy for Reversed {
    fn arrange(
        &self,
        _density: &Density,
        total_size: i32,
        sizes: &[i32],
        _layout_direction: LayoutDirection,
        out_positions: &mut [i32],
    ) {
        let mut end = total_size;
        for (index, size) in sizes.iter().enumerate() {
            end -= size;
            out_positions[index] = end;
        }
    }
}

#[composable]
fn App(expanded: bool) {
    let scroll = remember_scroll_state!();
    let _scroll2 = remember_scroll_state!(initial = 10);
    let _list = remember_lazy_list_state!();
    let _sheet = remember_modal_bottom_sheet_state!();
    let _sheet2 = remember_modal_bottom_sheet_state!(skip_partially_expanded = true);
    let _swipe = remember_swipe_to_dismiss_box_state!();
    let snackbars = remember(SnackbarHostState::new);
    let scope = remember_coroutine_scope();
    let width = 100.dp();
    let height = 40.dp();
    let custom = remember(|| HorizontalArrangement::custom(Reversed));
    Column!(
        modifier = Modifier.padding(8.dp()).vertical_scroll(scroll),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        {
            // Call forms.
            Box!(modifier = Modifier.padding((8.dp(), 4.dp())).size((width, height)));
            Box!(modifier = Modifier.padding((1.dp(), 2.dp(), 3.dp(), 4.dp())).size(10.dp()));
            Box!(
                modifier = Modifier.padding(PaddingValues { start: 8.dp(), ..PaddingValues::ZERO })
            );
            Box!(
                modifier =
                    Modifier.size(DpSize::new(width, height)).required_size((4.dp(), 4.dp()))
            );
            Box!(modifier = Modifier.background(Color::RED));
            Box!(
                modifier =
                    Modifier.background(Brush::horizontal_gradient([Color::RED, Color::BLUE]))
            );
            Box!(modifier = Modifier.background_with(Color::RED, RoundedCornerShape(8.dp())));
            Box!(modifier = Modifier.clip(RoundedCornerShape((1.dp(), 2.dp(), 3.dp(), 4.dp()))));
            Box!(modifier = Modifier.clip(RoundedCornerShape(50)));
            Spacer!();
            Spacer!(modifier = Modifier.height(8.dp()));
            // Arrangements.
            Row!(horizontal_arrangement = Arrangement::spaced_by((8.dp(), Alignment::END)), {});
            Row!(horizontal_arrangement = Arrangement::aligned(Alignment::CENTER_HORIZONTALLY), {});
            Row!(horizontal_arrangement = Arrangement::CENTER, {});
            Row!(horizontal_arrangement = custom.clone(), {
                Box!(modifier = Modifier.size(10.dp()).test_tag("first"));
                Box!(modifier = Modifier.size(10.dp()));
            });
            Row!(
                horizontal_arrangement =
                    if expanded { Arrangement::START } else { Arrangement::CENTER.into() },
                {}
            );
            Column!(vertical_arrangement = Arrangement::spaced_by((8.dp(), Alignment::BOTTOM)), {});
            LazyColumn!(
                modifier = Modifier.height(10.dp()),
                vertical_arrangement = Arrangement::SPACE_BETWEEN,
                |_scope: LazyListScope| {}
            );
            // Clickables.
            Box!(
                modifier = Modifier
                    .size(20.dp())
                    .toggleable_with(
                        true,
                        ClickableOptions { role: Some(Role::Switch), ..ClickableOptions::DEFAULT },
                        |_| {}
                    )
                    .test_tag("toggle")
            );
            // Custom layout.
            Layout!(
                modifier = marker(Modifier.test_tag("custom"), Color::BLUE),
                measure_policy = measure_policy(|scope, measurables, constraints| {
                    let mut x = 0;
                    for &measurable in measurables {
                        let placeable = scope.measure(measurable, constraints.loosen());
                        scope.place(&placeable, x, 0);
                        x += placeable.width;
                    }
                    scope.layout(x, 10)
                }),
                {
                    Box!(modifier = Modifier.size(20.dp()));
                }
            );
            Canvas(Modifier.size(10.dp()), |scope| {
                let paint = scope.brush_paint(
                    &Brush::linear_gradient_with(
                        [(0.0, Color::RED), (1.0, Color::BLUE)],
                        Offset::ZERO,
                        Offset::INFINITE,
                        TileMode::Clamp,
                    ),
                    1.0,
                    PaintStyle::Fill,
                );
                scope.draw_rect(Rect::new(0.0, 0.0, 1.0, 1.0), paint);
            });
            Button!(
                on_click = move || {
                    let snackbars = snackbars;
                    scope.launch(async move {
                        snackbars.show_snackbar("hello").await;
                        snackbars.show_snackbar(String::from("again")).await;
                        snackbars
                            .show_snackbar_with(
                                "undo?",
                                Some("Undo".into()),
                                true,
                                SnackbarDuration::Long,
                            )
                            .await;
                    });
                },
                {
                    Text!("Snack");
                }
            );
        }
    );
}

#[test]
fn the_app_composes_and_the_custom_arrangement_reverses() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 2000), || App(false));
    ui.on_node_with_tag("custom").assert_exists();
    assert_eq!(ui.on_node_with_tag("first").fetch_semantics_node().bounds.left, 18.0);
    let tree = ui.dump_tree();
    assert!(tree.contains("toggle"), "{tree}");
}
