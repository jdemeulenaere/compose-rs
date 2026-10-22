//! Phase D review: a randomized differential test of incremental measurement. A tree of
//! layouts (immutable specs shared by `Rc`, so unchanged subtrees skip) is edited at random:
//! modifier values, policies, parent data (weight, alignment), children inserted, removed and
//! moved, root resizes and density changes. After each frame the tree dump must equal a fresh
//! host's composing the final spec directly.

#![allow(non_snake_case)]

use crate::support;

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::{ColumnPolicy, RowPolicy};

#[derive(Clone, Debug)]
struct Spec {
    key: u32,
    policy: u8,
    width: Option<u8>,
    height: Option<u8>,
    padding: u8,
    offset: u8,
    fill: bool,
    required: Option<u8>,
    weight: Option<u8>,
    align: Option<u8>,
    children: Vec<Rc<Spec>>,
}

impl ComposeArg for Spec {
    fn same(&self, _: &Self) -> bool {
        false
    }
}

/// Children side by side; weighted ones share the width left by the others, as Row's weights.
struct WeightedRowPolicy;

impl MeasurePolicy for WeightedRowPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let mut sizes = vec![None; measurables.len()];
        let mut used = 0;
        let mut total = 0.0;
        for (index, &measurable) in measurables.iter().enumerate() {
            match scope.parent_data(measurable).weight {
                Some((weight, _)) => total += weight,
                None => {
                    let placeable = scope.measure(
                        measurable,
                        Constraints::new(0, Constraints::INFINITY, 0, constraints.max_height),
                    );
                    used += placeable.width;
                    sizes[index] = Some(placeable);
                }
            }
        }
        let width = if constraints.has_bounded_width() { constraints.max_width } else { used };
        let remaining = (width - used).max(0);
        for (index, &measurable) in measurables.iter().enumerate() {
            if let Some((weight, _)) = scope.parent_data(measurable).weight {
                let share = (remaining as f32 * weight / total) as i32;
                sizes[index] = Some(scope.measure(
                    measurable,
                    Constraints::new(share, share, 0, constraints.max_height),
                ));
            }
        }
        let (mut x, mut height) = (0, 0);
        for placeable in sizes.iter().flatten() {
            scope.place(placeable, x, 0);
            x += placeable.width;
            height = height.max(placeable.height);
        }
        scope.layout(constraints.constrain_width(x), constraints.constrain_height(height))
    }
}

/// Children stacked and aligned by their `align`, in the largest child's size.
struct AlignedBoxPolicy;

impl MeasurePolicy for AlignedBoxPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&m| scope.measure(m, constraints.loosen())).collect();
        let width = placeables.iter().map(|p| p.width).max().unwrap_or(0);
        let height = placeables.iter().map(|p| p.height).max().unwrap_or(0);
        let size = constraints.constrain(IntSize::new(width, height));
        for (placeable, &measurable) in placeables.iter().zip(measurables) {
            let alignment = match scope.parent_data(measurable).alignment {
                Some(ParentAlignment::Box(alignment)) => alignment,
                _ => Alignment::TOP_START,
            };
            let at = alignment.align(placeable.size(), size, LayoutDirection::Ltr);
            scope.place_at(placeable, at);
        }
        scope.layout(size.width, size.height)
    }
}

/// Measures and places its first child only.
struct FirstOnlyPolicy;

impl MeasurePolicy for FirstOnlyPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let Some(&first) = measurables.first() else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let placeable = scope.measure(first, constraints.loosen());
        scope.place(&placeable, 1, 1);
        scope.layout(
            constraints.constrain_width(placeable.width + 1),
            constraints.constrain_height(placeable.height + 1),
        )
    }
}

/// Measures every child but places only the even ones, in a column.
struct PlaceEvenPolicy;

impl MeasurePolicy for PlaceEvenPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let (mut width, mut y) = (0, 0);
        for (index, &measurable) in measurables.iter().enumerate() {
            let placeable = scope.measure(measurable, constraints.loosen());
            if index % 2 == 0 {
                scope.place(&placeable, 0, y);
                y += placeable.height;
                width = width.max(placeable.width);
            }
        }
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(y))
    }
}

thread_local! {
    static POLICIES: [Rc<dyn MeasurePolicy>; 7] = [
        Rc::new(ColumnPolicy),
        Rc::new(RowPolicy),
        Rc::new(StackMeasurePolicy),
        Rc::new(WeightedRowPolicy),
        Rc::new(AlignedBoxPolicy),
        Rc::new(FirstOnlyPolicy),
        Rc::new(PlaceEvenPolicy),
    ];
}

fn modifier_of(spec: &Spec) -> Modifier {
    let mut modifier = Modifier;
    if let Some(weight) = spec.weight {
        modifier = modifier.weight(RowScope::__new(), weight as f32 + 1.0);
    }
    if let Some(align) = spec.align {
        let alignment =
            [Alignment::CENTER, Alignment::BOTTOM_END, Alignment::TOP_CENTER][align as usize % 3];
        modifier = modifier.align(BoxScope::__new(), alignment);
    }
    // Outermost, so the node's own size may exceed its constraints.
    if let Some(required) = spec.required {
        modifier = modifier.required_size((required as f32).dp(), (required as f32).dp());
    }
    modifier =
        modifier.offset((spec.offset as f32).dp(), 0.dp()).padding((spec.padding as f32).dp());
    if spec.fill {
        modifier = modifier.fill_max_width_fraction(0.5);
    }
    match (spec.width, spec.height) {
        (Some(width), Some(height)) => modifier.size_wh((width as f32).dp(), (height as f32).dp()),
        (Some(width), None) => modifier.width((width as f32).dp()),
        (None, Some(height)) => modifier.height((height as f32).dp()),
        (None, None) => modifier,
    }
}

#[composable]
fn Render(spec: Rc<Spec>) {
    let policy = POLICIES.with(|policies| policies[spec.policy as usize % 7].clone());
    let children = spec.children.clone();
    Layout(
        modifier_of(&spec),
        policy,
        Callback::from_rc(Rc::new(move || {
            for child in &children {
                key(child.key, || Render(child.clone()));
            }
        }) as Rc<dyn Fn()>),
    );
}

struct Random(u64);

impl Random {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.next(100) < percent
    }
}

struct Model {
    random: Random,
    next_key: u32,
    allow_required: bool,
}

impl Model {
    fn leaf(&mut self) -> Spec {
        self.next_key += 1;
        Spec {
            key: self.next_key,
            policy: self.random.next(7) as u8,
            width: self.random.chance(60).then(|| self.random.next(60) as u8),
            height: self.random.chance(60).then(|| self.random.next(40) as u8),
            padding: if self.random.chance(30) { self.random.next(6) as u8 } else { 0 },
            offset: if self.random.chance(15) { self.random.next(6) as u8 } else { 0 },
            fill: self.random.chance(10),
            required: (self.allow_required && self.random.chance(10))
                .then(|| self.random.next(120) as u8),
            weight: self.random.chance(20).then(|| self.random.next(3) as u8),
            align: self.random.chance(20).then(|| self.random.next(3) as u8),
            children: Vec::new(),
        }
    }

    /// A copy of `spec` with one random edit somewhere below it.
    fn edit(&mut self, spec: &Spec, depth: usize) -> Spec {
        let mut spec = spec.clone();
        if !spec.children.is_empty() && depth < 5 && self.random.chance(65) {
            let index = self.random.next(spec.children.len());
            spec.children[index] = Rc::new(self.edit(&spec.children[index], depth + 1));
            return spec;
        }
        match self.random.next(9) {
            0 if spec.children.len() < 5 && depth < 5 => {
                let at = self.random.next(spec.children.len() + 1);
                let leaf = self.leaf();
                spec.children.insert(at, Rc::new(leaf));
            }
            1 if !spec.children.is_empty() => {
                let at = self.random.next(spec.children.len());
                spec.children.remove(at);
            }
            2 if spec.children.len() > 1 => {
                let from = self.random.next(spec.children.len());
                let child = spec.children.remove(from);
                let to = self.random.next(spec.children.len() + 1);
                spec.children.insert(to, child);
            }
            3 => spec.policy = self.random.next(7) as u8,
            4 => spec.width = self.random.chance(70).then(|| self.random.next(60) as u8),
            5 => spec.padding = self.random.next(6) as u8,
            6 => spec.weight = self.random.chance(50).then(|| self.random.next(3) as u8),
            7 => spec.align = self.random.chance(50).then(|| self.random.next(3) as u8),
            _ => {
                spec.fill = self.random.chance(30);
                spec.offset = self.random.next(4) as u8;
                if self.allow_required {
                    spec.required = self.random.chance(30).then(|| self.random.next(120) as u8);
                }
                spec.height = self.random.chance(70).then(|| self.random.next(40) as u8);
            }
        }
        spec
    }
}

/// The dump without the bounds of unplaced nodes and without their subtrees: they keep what
/// their last measurement left, which nothing reads (drawing, hit testing, focus and semantics
/// skip them).
fn normalized(dump: String) -> String {
    let mut out = String::new();
    let mut skip_deeper_than: Option<usize> = None;
    for line in dump.lines() {
        let depth = line.len() - line.trim_start().len();
        if let Some(limit) = skip_deeper_than {
            if depth > limit {
                continue;
            }
            skip_deeper_than = None;
        }
        if line.contains("(not placed)") {
            let open = line.find('[').expect("bounds");
            let close = line.find(']').expect("bounds");
            out.push_str(&format!("{}[unplaced]{}\n", &line[..open], &line[close + 1..]));
            skip_deeper_than = Some(depth);
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn fresh_dump(spec: Rc<Spec>, size: IntSize, density: Density) -> String {
    let mut host = UiHost::new(size, density, Box::new(FixedTextMeasurer::new()));
    host.set_content(move || Render(spec.clone()));
    host.frame();
    normalized(host.dump_tree())
}

fn run(seed: u64, rounds: usize, allow_required: bool) {
    let mut model = Model { random: Random(seed), next_key: 0, allow_required };
    let mut root = model.leaf();
    root.policy = 0;
    let spec = Rc::new(RefCell::new(Rc::new(root)));
    let version = mutable_state_of(0u32);
    let (shared, handle) = (spec.clone(), version.handle());
    let mut size = IntSize::new(200, 150);
    let mut density = Density::default();
    let mut host = UiHost::new(size, density, Box::new(FixedTextMeasurer::new()));
    host.set_content(move || {
        handle.get();
        let spec = shared.borrow().clone();
        Render(spec);
    });
    host.frame();
    for round in 0..rounds {
        let edits = 1 + model.random.next(3);
        for _ in 0..edits {
            let edited = model.edit(&spec.borrow(), 0);
            *spec.borrow_mut() = Rc::new(edited);
        }
        if model.random.chance(10) {
            size = IntSize::new(
                100 + model.random.next(200) as i32,
                80 + model.random.next(150) as i32,
            );
            host.resize(size);
        }
        if model.random.chance(5) {
            density = Density::new([1.0, 1.5, 2.0][model.random.next(3)]);
            host.set_density(density);
        }
        version.set(round as u32 + 1);
        host.frame();
        let expected = fresh_dump(spec.borrow().clone(), size, density);
        assert_eq!(
            normalized(host.dump_tree()),
            expected,
            "seed {seed}, round {round}: incremental layout differs from a fresh one"
        );
        fn count(spec: &Spec) -> usize {
            1 + spec.children.iter().map(|child| count(child)).sum::<usize>()
        }
        let nodes = 1 + count(&spec.borrow());
        assert_eq!(host.with_tree(|tree| tree.node_count()), nodes, "seed {seed}, round {round}");
    }
}

/// Without `required_size`: sound, keeps passing.
#[test]
fn incremental_layout_matches_a_fresh_layout() {
    for seed in 0..40 {
        run(seed, 150, false);
    }
}

/// With `required_size`, whose centering goes stale (see
/// review_d::a_required_size_change_in_a_fixed_parent_recenters_the_child).
#[test]
fn incremental_layout_matches_a_fresh_layout_with_required_sizes() {
    for seed in 0..40 {
        run(seed, 150, true);
    }
}
