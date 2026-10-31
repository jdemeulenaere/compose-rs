//! `Ruler`s, as Compose's `RulerTest` and `RectRulerTest`: what a layout provides to what's in it,
//! read while placing. Each with a block of values and with lazily provided ones, as its
//! `useIndividualRulers`. Density 1.

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

const BOTH: [bool; 2] = [false, true];

#[derive(Clone)]
struct Rulers {
    vertical: Ruler,
    horizontal: Ruler,
}

fn rulers() -> Rulers {
    Rulers { vertical: VerticalRuler::new(), horizontal: HorizontalRuler::new() }
}

type Value = Rc<dyn Fn(&RulerScope) -> Option<f32>>;

/// A layout modifier providing them.
fn provide(rulers: &Rulers, lazily: bool, horizontal: Value, vertical: Value) -> Modifier {
    let (v, h) = (rulers.vertical.clone(), rulers.horizontal.clone());
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (v, h, horizontal, vertical) =
            (v.clone(), h.clone(), horizontal.clone(), vertical.clone());
        let place = move |placement: &mut PlacementScope<'_>| placement.place(&placeable, 0, 0);
        let (width, height) = (scope_size(&placeable).0, scope_size(&placeable).1);
        if lazily {
            let (v2, h2) = (v.clone(), h.clone());
            scope.layout_with_ruler_provider(
                width,
                height,
                move |ruler| *ruler == v2 || *ruler == h2,
                move |rulers, ruler| {
                    let value = if *ruler == h { horizontal(rulers) } else { vertical(rulers) };
                    if let Some(value) = value {
                        rulers.provides(ruler, value);
                    }
                },
                place,
            )
        } else {
            scope.layout_with_rulers(
                width,
                height,
                move |rulers| {
                    if let Some(value) = vertical(rulers) {
                        rulers.provides(&v, value);
                    }
                    if let Some(value) = horizontal(rulers) {
                        rulers.provides(&h, value);
                    }
                },
                place,
            )
        }
    })
}

fn scope_size(placeable: &Placeable) -> (i32, i32) {
    (placeable.width, placeable.height)
}

fn fixed(value: f32) -> Value {
    Rc::new(move |_| Some(value))
}

/// A modifier moving what follows.
fn moved(x: i32, y: i32) -> Modifier {
    Modifier.absolute_offset_with(move |_| IntOffset::new(x, y))
}

type Read = Rc<RefCell<Vec<(f32, f32)>>>;

/// A layout modifier whose block keeps what it reads.
fn reader(rulers: &Rulers, read: &Read, default: f32) -> Modifier {
    let (v, h, read) = (rulers.vertical.clone(), rulers.horizontal.clone(), read.clone());
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (v, h, read) = (v.clone(), h.clone(), read.clone());
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            let values = (placement.current(&h, default), placement.current(&v, default));
            read.borrow_mut().push(values);
            placement.place(&placeable, 0, 0);
        })
    })
}

fn size(width: i32, height: i32) -> Modifier {
    Modifier.size(((width as f32).dp(), (height as f32).dp()))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueWithNoProvider
#[test]
fn with_no_provider_a_value_is_what_s_asked_for_nothing() {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let (r, re) = (rulers.clone(), read.clone());
    let _host = host(move || Leaf(reader(&r, &re, f32::NAN).then(size(10, 10))));
    let (h, v) = read.borrow()[0];
    assert!(h.is_nan() && v.is_nan());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueWithNoProviderDefaultValue
#[test]
fn with_no_provider_a_value_is_the_default() {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let (r, re) = (rulers.clone(), read.clone());
    let _host = host(move || Leaf(reader(&r, &re, 100.0).then(size(10, 10))));
    assert_eq!(read.borrow()[0], (100.0, 100.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueWithinLayout
#[test]
fn a_value_is_in_the_reader_s_coordinates() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let (r, re) = (rulers.clone(), read.clone());
        let _host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            Stack!(
                modifier = size(100, 150).then(provide(&r, lazily, fixed(53.0), fixed(35.0))),
                {
                    Leaf(moved(25, 50).then(reader(&r, &re, f32::NAN)).then(size(50, 50)));
                }
            );
        });
        assert_eq!(*read.borrow(), [(3.0, 10.0)], "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueNotWithinLayout
#[test]
fn a_value_outside_the_reader_is_a_position_outside_it() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let (r, re) = (rulers.clone(), read.clone());
        let _host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            Stack!(modifier = size(100, 150).then(provide(&r, lazily, fixed(7.0), fixed(5.0))), {
                Leaf(moved(25, 50).then(reader(&r, &re, f32::NAN)).then(size(50, 50)));
            });
        });
        let (h, v) = read.borrow()[0];
        assert!(h < 0.0 && v < 0.0, "lazily {lazily}");
    }
}

/// A layout providing them for itself: its block reads them too.
#[test]
fn a_layout_reads_what_it_provides() {
    let (rulers, seen) = (rulers(), Rc::new(Cell::new(0.0)));
    let (v, s) = (rulers.vertical.clone(), seen.clone());
    let _host = host(move || {
        let (v, s) = (v.clone(), s.clone());
        Leaf(Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (v, v2, s) = (v.clone(), v.clone(), s.clone());
            scope.layout_with_rulers(
                5,
                5,
                move |rulers| rulers.provides(&v, 3.0),
                move |placement| {
                    s.set(placement.current(&v2, -1.0));
                    placement.place(&placeable, 0, 0);
                },
            )
        }));
    });
    assert_eq!(seen.get(), 3.0);
}

fn relative(ruler: &Ruler, lazily: bool, value: f32) -> Modifier {
    let ruler = ruler.clone();
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (width, height) = (placeable.width, placeable.height);
        let (r, r2) = (ruler.clone(), ruler.clone());
        let place = move |placement: &mut PlacementScope<'_>| placement.place(&placeable, 0, 0);
        if lazily {
            scope.layout_with_ruler_provider(
                width,
                height,
                move |asked| *asked == r2,
                move |rulers, asked| rulers.provides_relative(asked, value),
                place,
            )
        } else {
            scope.layout_with_rulers(
                width,
                height,
                move |rulers| rulers.provides_relative(&r, value),
                place,
            )
        }
    })
}

/// A provider whose own direction is `direction`, and a reader 50 wide. Right to left, the stack
/// puts the reader at 50 and the offset at 75; left to right, at 25.
fn provided_relative(direction: LayoutDirection, lazily: bool) -> f32 {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let (r, re) = (rulers.clone(), read.clone());
    let _host = host(move || {
        let (r, re) = (r.clone(), re.clone());
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
            let (r, re) = (r.clone(), re.clone());
            Stack!(modifier = size(100, 150).then(relative(&r.vertical, lazily, 0.0)), {
                let (r, re) = (r.clone(), re.clone());
                CompositionLocalProvider(
                    LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Ltr),
                    move || {
                        Leaf(moved(25, 50).then(reader(&r, &re, f32::NAN)).then(size(50, 50)));
                    },
                );
            });
        });
    });
    read.borrow()[0].1
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueRtl
#[test]
fn a_relative_value_is_from_the_right_right_to_left() {
    for lazily in BOTH {
        // 0 from the right is 100, and the reader's at 75.
        assert_eq!(provided_relative(LayoutDirection::Rtl, lazily), 25.0, "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueLtr
#[test]
fn a_relative_value_is_from_the_left_left_to_right() {
    for lazily in BOTH {
        assert_eq!(provided_relative(LayoutDirection::Ltr, lazily), -25.0, "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, valueLayoutDirectionChanges
#[test]
fn a_relative_value_follows_its_provider_s_direction() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let direction = mutable_state_of(LayoutDirection::Ltr);
        let (r, re, d) = (rulers.clone(), read.clone(), direction.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(d.get()), move || {
                let (r, re) = (r.clone(), re.clone());
                Stack!(modifier = size(100, 150).then(relative(&r.vertical, lazily, 0.0)), {
                    Leaf(reader(&r, &re, f32::NAN).then(size(100, 150)));
                });
            });
        });
        assert_eq!(read.borrow().last().map(|it| it.1), Some(0.0));
        direction.set(LayoutDirection::Rtl);
        host.frame();
        assert_eq!(read.borrow().last().map(|it| it.1), Some(100.0), "lazily {lazily}");
    }
}

/// The nearest provider's, and the next one's when that one goes.
#[test]
fn the_nearest_provider_gives_the_value() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let inner = mutable_state_of(true);
        let (r, re, i) = (rulers.clone(), read.clone(), inner.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            Stack!(modifier = size(100, 100).then(provide(&r, lazily, fixed(1.0), fixed(2.0))), {
                let (r, re) = (r.clone(), re.clone());
                let (h, v): (Value, Value) = if i.get() {
                    (fixed(3.0), fixed(4.0))
                } else {
                    (Rc::new(|_| None), Rc::new(|_| None))
                };
                Stack!(modifier = provide(&r, lazily, h.clone(), v.clone()), {
                    Leaf(reader(&r, &re, f32::NAN).then(size(5, 5)));
                });
            });
        });
        assert_eq!(read.borrow().last().copied(), Some((3.0, 4.0)));
        inner.set(false);
        host.frame();
        assert_eq!(read.borrow().last().copied(), Some((1.0, 2.0)), "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, updateStateValue
#[test]
fn a_state_a_value_reads_tells_its_readers() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let value = mutable_state_of(35.0f32);
        let (r, re, va) = (rulers.clone(), read.clone(), value.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            let vertical: Value = Rc::new(move |_| Some(va.get()));
            Stack!(modifier = size(100, 150).then(provide(&r, lazily, fixed(53.0), vertical)), {
                Leaf(moved(25, 50).then(reader(&r, &re, f32::NAN)).then(size(50, 50)));
            });
        });
        assert_eq!(*read.borrow(), [(3.0, 10.0)]);
        value.set(36.0);
        host.frame();
        assert_eq!(*read.borrow(), [(3.0, 10.0), (3.0, 11.0)], "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, rulerValueRemoved
#[test]
fn a_value_gone_tells_its_readers() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let set = mutable_state_of(true);
        let (r, re, s) = (rulers.clone(), read.clone(), set.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            let h: Value = Rc::new(move |_| s.get().then_some(53.0));
            let v: Value = Rc::new(move |_| s.get().then_some(35.0));
            Stack!(modifier = size(100, 150).then(provide(&r, lazily, h, v)), {
                Leaf(moved(25, 50).then(reader(&r, &re, f32::NAN)).then(size(50, 50)));
            });
        });
        assert_eq!(*read.borrow(), [(3.0, 10.0)]);
        set.set(false);
        host.frame();
        let read = read.borrow();
        assert_eq!(read.len(), 2, "lazily {lazily}");
        assert!(read[1].0.is_nan() && read[1].1.is_nan());
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, rulerValueAdded
#[test]
fn a_value_come_tells_its_readers() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let set = mutable_state_of(false);
        let (r, re, s) = (rulers.clone(), read.clone(), set.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            let h: Value = Rc::new(move |_| s.get().then_some(53.0));
            let v: Value = Rc::new(move |_| s.get().then_some(35.0));
            Stack!(modifier = size(100, 150).then(provide(&r, lazily, h, v)), {
                Leaf(moved(25, 50).then(reader(&r, &re, f32::NAN)).then(size(50, 50)));
            });
        });
        assert!(read.borrow()[0].0.is_nan());
        set.set(true);
        host.frame();
        assert_eq!(read.borrow().last().copied(), Some((3.0, 10.0)), "lazily {lazily}");
    }
}

/// A provider coming in a recomposition, above a reader of nothing.
#[test]
fn a_provider_come_tells_its_readers() {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let on = mutable_state_of(false);
    let (r, re, o) = (rulers.clone(), read.clone(), on.handle());
    let mut host = host(move || {
        let (r, re) = (r.clone(), re.clone());
        let modifier =
            if o.get() { provide(&r, false, fixed(53.0), fixed(35.0)) } else { Modifier };
        Stack!(modifier = size(100, 150).then(modifier), {
            Leaf(reader(&r, &re, -1.0).then(size(50, 50)));
        });
    });
    assert_eq!(read.borrow()[0], (-1.0, -1.0));
    on.set(true);
    host.frame();
    assert_eq!(read.borrow().last().copied(), Some((53.0, 35.0)));
    on.set(false);
    host.frame();
    assert_eq!(read.borrow().last().copied(), Some((-1.0, -1.0)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, rulerChildMoved
#[test]
fn a_reader_moved_reads_again() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let offset = mutable_state_of((0, 0));
        let (r, re, o) = (rulers.clone(), read.clone(), offset.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            Stack!(
                modifier = size(100, 150).then(provide(&r, lazily, fixed(53.0), fixed(35.0))),
                {
                    Leaf(
                        Modifier
                            .offset_with(move |_| {
                                let (x, y) = o.get();
                                IntOffset::new(x, y)
                            })
                            .then(moved(25, 50))
                            .then(reader(&r, &re, f32::NAN))
                            .then(size(50, 50)),
                    );
                }
            );
        });
        assert_eq!(*read.borrow(), [(3.0, 10.0)]);
        offset.set((1, 2));
        host.frame();
        assert_eq!(*read.borrow(), [(3.0, 10.0), (1.0, 9.0)], "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, verticalDerivedRuler
#[test]
fn a_derived_vertical_ruler_gives_what_it_works_out() {
    let derived = VerticalRuler::derived(|_, _| 10.0);
    let seen = Rc::new(Cell::new(0.0));
    let s = seen.clone();
    let _host = host(move || {
        let (d, s) = (derived.clone(), s.clone());
        Leaf(Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (d, s) = (d.clone(), s.clone());
            scope.layout_with(5, 5, move |placement| {
                s.set(placement.current(&d, 0.0));
                placement.place(&placeable, 0, 0);
            })
        }));
    });
    assert_eq!(seen.get(), 10.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, horizontalDerivedRuler
#[test]
fn a_derived_horizontal_ruler_gets_its_default() {
    let derived = HorizontalRuler::derived(|_, default| default + 1.0);
    let seen = Rc::new(Cell::new(0.0));
    let s = seen.clone();
    let _host = host(move || {
        let (d, s) = (derived.clone(), s.clone());
        Leaf(Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (d, s) = (d.clone(), s.clone());
            scope.layout_with(5, 5, move |placement| {
                s.set(placement.current(&d, 9.0));
                placement.place(&placeable, 0, 0);
            })
        }));
    });
    assert_eq!(seen.get(), 10.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, readOnlyImportantRulers
#[test]
fn lazily_provided_only_what_s_read_is_worked_out() {
    for lazily in BOTH {
        let (horizontal, vertical) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let rulers = rulers();
        let (r, ho, ve) = (rulers.clone(), horizontal.clone(), vertical.clone());
        let _host = host(move || {
            let (r, ho, ve) = (r.clone(), ho.clone(), ve.clone());
            let h: Value = Rc::new(move |_| {
                ho.set(true);
                Some(35.0)
            });
            let v: Value = Rc::new(move |_| {
                ve.set(true);
                Some(53.0)
            });
            Stack!(modifier = provide(&r, lazily, h, v), {
                let ruler = r.horizontal.clone();
                Leaf(Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let ruler = ruler.clone();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        let x = placement.current(&ruler, f32::NAN);
                        placement.place(&placeable, if x.is_nan() { 0 } else { x as i32 }, 0);
                    })
                }));
            });
        });
        assert!(horizontal.get());
        assert_eq!(vertical.get(), !lazily);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, repeatedReadsWithoutValueChange_stillInvalidateOnChange
#[test]
fn reads_again_and_again_still_hear_a_change() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let (value, again) = (mutable_state_of(35.0f32), mutable_state_of(0));
        let (r, re, va, a) = (rulers.clone(), read.clone(), value.handle(), again.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            let vertical: Value = Rc::new(move |_| Some(va.get()));
            Stack!(modifier = size(100, 150).then(provide(&r, lazily, fixed(53.0), vertical)), {
                let (v, re) = (r.vertical.clone(), re.clone());
                Leaf(Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let (v, re) = (v.clone(), re.clone());
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        a.get();
                        let x = placement.current(&v, f32::NAN);
                        re.borrow_mut().push((0.0, x));
                        placement.place(&placeable, 0, 0);
                    })
                }));
            });
        });
        for round in 1..21 {
            again.set(round);
            host.frame();
        }
        assert_eq!(read.borrow().len(), 21, "lazily {lazily}");
        value.set(34.0);
        host.frame();
        assert_eq!(read.borrow().len(), 22, "lazily {lazily}");
        assert_eq!(read.borrow().last().map(|it| it.1), Some(34.0));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, removedReader_notReplacedOnValueChange
#[test]
fn a_reader_gone_hears_nothing() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let (value, show) = (mutable_state_of(35.0f32), mutable_state_of(true));
        let (r, re, va, sh) = (rulers.clone(), read.clone(), value.handle(), show.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            let vertical: Value = Rc::new(move |_| Some(va.get()));
            Stack!(modifier = size(100, 150).then(provide(&r, lazily, fixed(53.0), vertical)), {
                if sh.get() {
                    Leaf(reader(&r, &re, f32::NAN).then(size(5, 5)));
                }
            });
        });
        assert_eq!(read.borrow().len(), 1);
        show.set(false);
        host.frame();
        value.set(34.0);
        host.frame();
        assert_eq!(read.borrow().len(), 1, "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, sideEffectRulersInvalidate_coordinateChange
#[test]
fn a_value_from_the_provider_s_position_follows_it() {
    for lazily in BOTH {
        let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
        let offset = mutable_state_of(0);
        let (r, re, o) = (rulers.clone(), read.clone(), offset.handle());
        let mut host = host(move || {
            let (r, re) = (r.clone(), re.clone());
            let vertical: Value =
                Rc::new(|scope: &RulerScope| Some(-scope.coordinates().position_in_root().x));
            let none: Value = Rc::new(|_| None);
            Stack!(modifier = Modifier.offset_with(move |_| IntOffset::new(o.get(), 0)), {
                let (r, re, vertical, none) =
                    (r.clone(), re.clone(), vertical.clone(), none.clone());
                Stack!(modifier = provide(&r, lazily, none.clone(), vertical.clone()), {
                    Leaf(reader(&r, &re, f32::NAN).then(size(5, 5)));
                });
            });
        });
        assert_eq!(read.borrow().last().map(|it| it.1), Some(0.0));
        offset.set(7);
        host.frame();
        assert_eq!(read.borrow().last().map(|it| it.1), Some(-7.0), "lazily {lazily}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, rulerMovesWithLayout
#[test]
fn a_reader_placing_nothing_still_follows_its_provider() {
    for lazily in BOTH {
        let rulers = rulers();
        let (offset, value) = (mutable_state_of(0), Rc::new(Cell::new(1.0)));
        let (r, v, o) = (rulers.clone(), value.clone(), offset.handle());
        let mut host = host(move || {
            let (r, v) = (r.clone(), v.clone());
            let vertical: Value =
                Rc::new(|scope: &RulerScope| Some(-scope.coordinates().position_in_root().x));
            let none: Value = Rc::new(|_| None);
            Stack!(modifier = Modifier.offset_with(move |_| IntOffset::new(o.get(), 0)), {
                let (r, v, vertical, none) = (r.clone(), v.clone(), vertical.clone(), none.clone());
                Stack!(modifier = provide(&r, lazily, none.clone(), vertical.clone()), {
                    let (ruler, v) = (r.vertical.clone(), v.clone());
                    Leaf(Modifier.layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let (ruler, v) = (ruler.clone(), v.clone());
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            v.set(placement.current(&ruler, f32::NAN));
                        })
                    }));
                });
            });
        });
        assert_eq!(value.get(), 0.0);
        offset.set(100);
        host.frame();
        assert_eq!(value.get(), -100.0, "lazily {lazily}");
    }
}

fn keep(
    ruler: &Ruler,
    seen: &Rc<Cell<f32>>,
    reads: impl Fn() -> bool + Clone + 'static,
) -> Modifier {
    let (ruler, seen) = (ruler.clone(), seen.clone());
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (ruler, seen, reads) = (ruler.clone(), seen.clone(), reads.clone());
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if reads() {
                seen.set(placement.current(&ruler, 0.0));
            }
            placement.place(&placeable, 0, 0);
        })
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, sideEffectRulersInvalidate
#[test]
fn a_state_a_lazy_value_reads_tells_who_read_it() {
    let rulers = rulers();
    let value = mutable_state_of(10.0f32);
    let (seen1, seen2) = (Rc::new(Cell::new(0.0)), Rc::new(Cell::new(0.0)));
    let (r, va, s1, s2) = (rulers.clone(), value.handle(), seen1.clone(), seen2.clone());
    let mut host = host(move || {
        let (a, b) = (r.vertical.clone(), r.horizontal.clone());
        let (a2, b2) = (a.clone(), b.clone());
        let (a3, b3) = (a.clone(), b.clone());
        let (s1, s2) = (s1.clone(), s2.clone());
        let provider = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (width, height) = (placeable.width, placeable.height);
            let (a, b, a2, b2) = (a.clone(), b.clone(), a2.clone(), b2.clone());
            scope.layout_with_ruler_provider(
                width,
                height,
                move |ruler| *ruler == a || *ruler == b,
                move |scope, _| {
                    scope.provides(&a2, va.get());
                    scope.provides(&b2, va.get());
                },
                move |placement| placement.place(&placeable, 0, 0),
            )
        });
        Stack!(modifier = provider, {
            Leaf(keep(&a3, &s1, || true));
            Leaf(keep(&b3, &s2, || true));
        });
    });
    assert_eq!((seen1.get(), seen2.get()), (10.0, 10.0));
    value.set(20.0);
    host.frame();
    assert_eq!((seen1.get(), seen2.get()), (20.0, 20.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, nestedSideEffectRulersInvalidate
#[test]
fn a_lazy_provider_giving_others_too_gives_what_s_asked_now() {
    let (a, b, c) = (VerticalRuler::new(), HorizontalRuler::new(), VerticalRuler::new());
    let (first, second) = (mutable_state_of(10.0f32), mutable_state_of(10.0f32));
    let (read_a, read_b, read_c) =
        (mutable_state_of(false), mutable_state_of(false), mutable_state_of(false));
    let (seen_a, seen_b, seen_c) =
        (Rc::new(Cell::new(0.0)), Rc::new(Cell::new(0.0)), Rc::new(Cell::new(0.0)));
    let (fi, se) = (first.handle(), second.handle());
    let (ra, rb, rc) = (read_a.handle(), read_b.handle(), read_c.handle());
    let (sa, sb, sc) = (seen_a.clone(), seen_b.clone(), seen_c.clone());
    let mut host = host(move || {
        let (a, b, c) = (a.clone(), b.clone(), c.clone());
        let (a2, b2, c2) = (a.clone(), b.clone(), c.clone());
        let (a3, b3, c3) = (a.clone(), b.clone(), c.clone());
        let (sa, sb, sc) = (sa.clone(), sb.clone(), sc.clone());
        let provider = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (width, height) = (placeable.width, placeable.height);
            let (a, b, c) = (a.clone(), b.clone(), c.clone());
            let (a2, b2, c2) = (a2.clone(), b2.clone(), c2.clone());
            scope.layout_with_ruler_provider(
                width,
                height,
                move |ruler| *ruler == a || *ruler == b || *ruler == c,
                move |scope, ruler| {
                    if *ruler == a2 {
                        scope.provides(&a2, fi.get());
                        scope.provides(&b2, fi.get());
                    } else if *ruler == b2 || *ruler == c2 {
                        scope.provides(&b2, se.get());
                        scope.provides(&c2, se.get());
                    }
                },
                move |placement| placement.place(&placeable, 0, 0),
            )
        });
        Stack!(modifier = provider, {
            Leaf(keep(&a3, &sa, move || ra.get()));
            Leaf(keep(&b3, &sb, move || rb.get()));
            Leaf(keep(&c3, &sc, move || rc.get()));
        });
    });
    read_a.set(true);
    host.frame();
    assert_eq!(seen_a.get(), 10.0);
    read_a.set(false);
    first.set(20.0);
    host.frame();
    read_b.set(true);
    host.frame();
    assert_eq!(seen_b.get(), 10.0);
    read_b.set(false);
    read_c.set(true);
    host.frame();
    assert_eq!(seen_c.get(), 10.0);
    read_a.set(true);
    host.frame();
    assert_eq!(seen_a.get(), 20.0);
    first.set(30.0);
    second.set(40.0);
    host.frame();
    assert_eq!(seen_c.get(), 40.0);
    assert_eq!(seen_a.get(), 30.0);
}

/// A reader beside its provider isn't told of what's only in that provider.
#[test]
fn a_change_tells_only_who_s_under_the_provider() {
    let (rulers, inside, beside): (Rulers, Read, Read) = (rulers(), Rc::default(), Rc::default());
    let value = mutable_state_of(1.0f32);
    let (r, i, b, va) = (rulers.clone(), inside.clone(), beside.clone(), value.handle());
    let mut host = host(move || {
        let (r, i, b) = (r.clone(), i.clone(), b.clone());
        Stack!(modifier = size(100, 100), {
            let (r2, i) = (r.clone(), i.clone());
            let vertical: Value = Rc::new(move |_| Some(va.get()));
            Stack!(modifier = provide(&r2, false, fixed(0.0), vertical), {
                Leaf(reader(&r2, &i, f32::NAN).then(size(5, 5)));
            });
            Leaf(reader(&r, &b, f32::NAN).then(size(5, 5)));
        });
    });
    assert_eq!((inside.borrow().len(), beside.borrow().len()), (1, 1));
    value.set(2.0);
    host.frame();
    assert_eq!((inside.borrow().len(), beside.borrow().len()), (2, 1));
    assert!(beside.borrow()[0].1.is_nan());
}

/// Told once it's no reader, and not after.
#[test]
fn a_reader_that_stopped_reading_is_told_once() {
    let rulers = rulers();
    let (value, reads) = (mutable_state_of(1.0f32), mutable_state_of(true));
    let places = Rc::new(Cell::new(0));
    let (r, va, re, p) = (rulers.clone(), value.handle(), reads.handle(), places.clone());
    let mut host = host(move || {
        let (r2, p) = (r.clone(), p.clone());
        let vertical: Value = Rc::new(move |_| Some(va.get()));
        Stack!(modifier = provide(&r, false, fixed(0.0), vertical), {
            let (ruler, p) = (r2.vertical.clone(), p.clone());
            Leaf(Modifier.layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let (ruler, p) = (ruler.clone(), p.clone());
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    p.set(p.get() + 1);
                    if re.get() {
                        placement.current(&ruler, 0.0);
                    }
                    placement.place(&placeable, 0, 0);
                })
            }));
        });
    });
    assert_eq!(places.get(), 1);
    reads.set(false);
    host.frame();
    assert_eq!(places.get(), 2);
    value.set(2.0);
    host.frame();
    assert_eq!(places.get(), 3, "what it read last time changed");
    value.set(3.0);
    host.frame();
    assert_eq!(places.get(), 3);
}

/// A measure asked for by a state, what it gives by what it reads.
fn measured_provider(
    rulers: &Rulers,
    state: MutableState<i32>,
    give: impl Fn(i32) -> Option<f32> + Clone + 'static,
    lazily: bool,
) -> Modifier {
    let ruler = rulers.vertical.clone();
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (width, height) = (placeable.width, placeable.height);
        let place = move |placement: &mut PlacementScope<'_>| placement.place(&placeable, 0, 0);
        let Some(value) = give(state.get()) else { return scope.layout_with(width, height, place) };
        let (ruler, ruler2) = (ruler.clone(), ruler.clone());
        if lazily {
            scope.layout_with_ruler_provider(
                width,
                height,
                move |asked| *asked == ruler2,
                move |rulers, asked| rulers.provides(asked, value),
                place,
            )
        } else {
            scope.layout_with_rulers(width, height, move |it| it.provides(&ruler, value), place)
        }
    })
}

fn measured(
    lazily: bool,
    give: impl Fn(i32) -> Option<f32> + Clone + 'static,
) -> (compose_ui::platform::UiHost, Read, OwnedState<i32>) {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let state = mutable_state_of(0);
    let (r, re, st) = (rulers.clone(), read.clone(), state.handle());
    let host = host(move || {
        let (r, re) = (r.clone(), re.clone());
        Stack!(modifier = measured_provider(&r, st, give.clone(), lazily), {
            Leaf(reader(&r, &re, f32::NAN).then(size(5, 5)));
        });
    });
    (host, read, state)
}

/// A measurement that gives none where the last gave them takes them away.
#[test]
fn a_measurement_giving_none_tells_the_readers() {
    for lazily in BOTH {
        let (mut host, read, state) = measured(lazily, |at| (at == 0).then_some(5.0));
        assert_eq!(read.borrow().last().map(|it| it.1), Some(5.0));
        state.set(1);
        host.frame();
        assert!(read.borrow().last().is_some_and(|it| it.1.is_nan()), "lazily {lazily}");
    }
}

/// A measurement giving a block of the same values tells no one.
#[test]
fn a_measurement_giving_the_same_values_tells_no_reader() {
    let (mut host, read, state) = measured(false, |_| Some(5.0));
    state.set(1);
    host.frame();
    assert_eq!(read.borrow().len(), 1);
}

/// One worked out when read, as a new measurement gives it, may be anything: they read again.
#[test]
fn a_measurement_giving_a_new_provider_tells_the_readers() {
    let (mut host, read, state) = measured(true, |at| Some(at as f32));
    state.set(1);
    host.frame();
    assert_eq!(read.borrow().last().map(|it| it.1), Some(1.0));
}

/// Not a number is no value: the next one out gives it.
#[test]
fn a_value_that_s_not_a_number_is_the_next_provider_s() {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let (r, re) = (rulers.clone(), read.clone());
    let _host = host(move || {
        let (r, re) = (r.clone(), re.clone());
        Stack!(modifier = provide(&r, false, fixed(1.0), fixed(2.0)), {
            let (r, re) = (r.clone(), re.clone());
            Stack!(modifier = provide(&r, false, fixed(f32::NAN), fixed(f32::NAN)), {
                Leaf(reader(&r, &re, f32::NAN).then(size(5, 5)));
            });
        });
    });
    assert_eq!(read.borrow()[0], (1.0, 2.0));
}

/// A chain shorter by layers: what those gave is gone. The reader's the same one, composed again
/// but with what it had, so only being told places it.
#[test]
fn a_layer_gone_takes_its_rulers_with_it() {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let on = mutable_state_of(true);
    let child = reader(&rulers, &read, -1.0).then(size(5, 5));
    let (r, o) = (rulers.clone(), on.handle());
    let mut host = host(move || {
        let modifier = if o.get() {
            moved(0, 0).then(moved(0, 0)).then(provide(&r, false, fixed(1.0), fixed(2.0)))
        } else {
            Modifier
        };
        let child = child.clone();
        Stack!(modifier = modifier, {
            Leaf(child.clone());
        });
    });
    assert_eq!(read.borrow().len(), 1);
    assert_eq!(read.borrow().last().copied(), Some((1.0, 2.0)));
    on.set(false);
    host.frame();
    assert_eq!(read.borrow().len(), 2);
    assert_eq!(read.borrow().last().copied(), Some((-1.0, -1.0)));
    on.set(true);
    host.frame();
    assert_eq!(read.borrow().len(), 3);
    assert_eq!(read.borrow().last().copied(), Some((1.0, 2.0)));
}

/// A measurement giving them where the last gave none tells who read nothing.
#[test]
fn a_measurement_giving_them_tells_the_readers() {
    for lazily in BOTH {
        let (mut host, read, state) = measured(lazily, |at| (at == 1).then_some(5.0));
        assert!(read.borrow()[0].1.is_nan());
        state.set(1);
        host.frame();
        assert_eq!(read.borrow().last().map(|it| it.1), Some(5.0), "lazily {lazily}");
    }
}

/// A node giving a value no state holds, asked to look again.
struct NonState {
    ruler: Ruler,
    lazily: bool,
    value: Rc<Cell<f32>>,
    handle: Rc<Cell<Option<NodeHandle>>>,
}

impl ModifierNode for NonState {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle.set(Some(context.handle()));
    }
}

impl LayoutModifierNode for NonState {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        let (ruler, ruler2, value) = (self.ruler.clone(), self.ruler.clone(), self.value.clone());
        let (width, height) = (placeable.width, placeable.height);
        let place = move |placement: &mut PlacementScope<'_>| placement.place(&placeable, 0, 0);
        if self.lazily {
            scope.layout_with_ruler_provider(
                width,
                height,
                move |asked| *asked == ruler2,
                move |rulers, asked| rulers.provides(asked, value.get()),
                place,
            )
        } else {
            scope.layout_with_rulers(
                width,
                height,
                move |rulers| rulers.provides(&ruler, value.get()),
                place,
            )
        }
    }
}

struct NonStateElement {
    ruler: Ruler,
    lazily: bool,
    value: Rc<Cell<f32>>,
    handle: Rc<Cell<Option<NodeHandle>>>,
}

impl ModifierNodeElement for NonStateElement {
    type Node = NonState;
    fn create(&self) -> NonState {
        NonState {
            ruler: self.ruler.clone(),
            lazily: self.lazily,
            value: self.value.clone(),
            handle: self.handle.clone(),
        }
    }
    fn update(&self, _: &mut NonState) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

fn non_state(lazily: bool) -> (compose_ui::platform::UiHost, Read, Rc<Cell<f32>>, NodeHandle) {
    let (rulers, read): (Rulers, Read) = (rulers(), Rc::default());
    let value = Rc::new(Cell::new(10.0));
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let (r, re, va, h) = (rulers.clone(), read.clone(), value.clone(), handle.clone());
    let host = host(move || {
        let (r, re) = (r.clone(), re.clone());
        let element = NonStateElement {
            ruler: r.vertical.clone(),
            lazily,
            value: va.clone(),
            handle: h.clone(),
        };
        Stack!(modifier = Modifier.then_custom(element), {
            Leaf(reader(&r, &re, f32::NAN).then(size(5, 5)));
        });
    });
    let handle = handle.get().expect("attached");
    (host, read, value, handle)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, invalidateRulersRecalculatesNonStateValue
#[test]
fn invalidate_rulers_works_them_out_again() {
    for lazily in BOTH {
        let (mut host, read, value, handle) = non_state(lazily);
        assert_eq!(read.borrow().last().map(|it| it.1), Some(10.0));
        value.set(20.0);
        host.frame();
        assert_eq!(read.borrow().len(), 1, "not observed");
        handle.invalidate_rulers();
        host.frame();
        assert_eq!(read.borrow().len(), 2, "lazily {lazily}");
        assert_eq!(read.borrow().last().map(|it| it.1), Some(20.0));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, invalidateRulersUnchangedValueDoesNotRelayoutReaders
#[test]
fn invalidate_rulers_with_the_same_values_tells_no_reader() {
    let (mut host, read, _value, handle) = non_state(false);
    assert_eq!(read.borrow().len(), 1);
    handle.invalidate_rulers();
    host.frame();
    assert_eq!(read.borrow().len(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RulerTest.kt, invalidateRulersWhenDetachedDoesNotCrash
#[test]
fn invalidate_rulers_detached_does_nothing() {
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let show = mutable_state_of(true);
    let (h, sh) = (handle.clone(), show.handle());
    let ruler = VerticalRuler::new();
    let mut host = host(move || {
        if sh.get() {
            let element = NonStateElement {
                ruler: ruler.clone(),
                lazily: false,
                value: Rc::new(Cell::new(1.0)),
                handle: h.clone(),
            };
            Leaf(Modifier.then_custom(element));
        }
    });
    let handle = handle.get().expect("attached");
    show.set(false);
    host.frame();
    handle.invalidate_rulers();
    host.frame();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RectRulerTest.kt, defaultRectRulers
#[test]
fn rect_rulers_are_four_rulers() {
    let rulers = RectRulers::new();
    let seen = Rc::new(Cell::new(Rect::new(0.0, 0.0, 0.0, 0.0)));
    let (r, s) = (rulers.clone(), seen.clone());
    let _host = host(move || {
        let (r, r2, s) = (r.clone(), r.clone(), s.clone());
        Leaf(
            size(100, 100)
                .then(Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let r = r.clone();
                    scope.layout_with_rulers(
                        placeable.width,
                        placeable.height,
                        move |rulers| {
                            rulers.provides(&r.left, 10.0);
                            rulers.provides(&r.top, 20.0);
                            rulers.provides(&r.right, 30.0);
                            rulers.provides(&r.bottom, 40.0);
                        },
                        move |placement| placement.place(&placeable, 0, 0),
                    )
                }))
                .then(rect_reader(&r2, &s)),
        );
    });
    assert_eq!(seen.get(), Rect::new(10.0, 20.0, 30.0, 40.0));
}

fn rect_reader(rulers: &RectRulers, seen: &Rc<Cell<Rect>>) -> Modifier {
    let (r, s) = (rulers.clone(), seen.clone());
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (r, s) = (r.clone(), s.clone());
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            s.set(Rect::new(
                placement.current(&r.left, f32::NAN),
                placement.current(&r.top, f32::NAN),
                placement.current(&r.right, f32::NAN),
                placement.current(&r.bottom, f32::NAN),
            ));
            placement.place(&placeable, 0, 0);
        })
    })
}

fn rects_provider(rulers: Vec<(RectRulers, Rect)>) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let rulers = rulers.clone();
        scope.layout_with_rulers(
            placeable.width,
            placeable.height,
            move |scope| {
                for (ruler, rect) in &rulers {
                    scope.provides(&ruler.left, rect.left);
                    scope.provides(&ruler.top, rect.top);
                    scope.provides(&ruler.right, rect.right);
                    scope.provides(&ruler.bottom, rect.bottom);
                }
            },
            move |placement| placement.place(&placeable, 0, 0),
        )
    })
}

fn of_two(combined: impl Fn(&[RectRulers]) -> RectRulers) -> Rect {
    let (a, b) = (RectRulers::new(), RectRulers::new());
    let combined = combined(&[a.clone(), b.clone()]);
    let seen = Rc::new(Cell::new(Rect::new(0.0, 0.0, 0.0, 0.0)));
    let s = seen.clone();
    let _host = host(move || {
        Leaf(
            size(100, 100)
                .then(rects_provider(vec![
                    (a.clone(), Rect::new(10.0, 20.0, 30.0, 40.0)),
                    (b.clone(), Rect::new(15.0, 5.0, 35.0, 25.0)),
                ]))
                .then(rect_reader(&combined, &s)),
        );
    });
    seen.get()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RectRulerTest.kt, innerRectRulers
#[test]
fn the_innermost_of_rect_rulers() {
    assert_eq!(of_two(RectRulers::innermost_of), Rect::new(15.0, 20.0, 30.0, 25.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RectRulerTest.kt, outerRectRulers
#[test]
fn the_outermost_of_rect_rulers() {
    assert_eq!(of_two(RectRulers::outermost_of), Rect::new(10.0, 5.0, 35.0, 40.0));
}

/// Two readers of two rulers, one's value changing: only its reader's told.
#[test]
fn one_value_changing_tells_only_who_read_it() {
    for lazily in BOTH {
        let (rulers, first, second): (Rulers, Read, Read) =
            (rulers(), Rc::default(), Rc::default());
        let value = mutable_state_of(1.0f32);
        let (r, a, b, va) = (rulers.clone(), first.clone(), second.clone(), value.handle());
        let mut host = host(move || {
            let (r, a, b) = (r.clone(), a.clone(), b.clone());
            let vertical: Value = Rc::new(move |_| Some(va.get()));
            Stack!(modifier = provide(&r, lazily, fixed(5.0), vertical), {
                let (h, v) = (r.horizontal.clone(), r.vertical.clone());
                let reads = |ruler: Ruler, read: Read| {
                    Modifier.layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let (ruler, read) = (ruler.clone(), read.clone());
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            let at = placement.current(&ruler, f32::NAN);
                            read.borrow_mut().push((at, at));
                            placement.place(&placeable, 0, 0);
                        })
                    })
                };
                Leaf(reads(v, a.clone()).then(size(5, 5)));
                Leaf(reads(h, b.clone()).then(size(5, 5)));
            });
        });
        assert_eq!((first.borrow().len(), second.borrow().len()), (1, 1));
        value.set(2.0);
        host.frame();
        assert_eq!((first.borrow().len(), second.borrow().len()), (2, 1), "lazily {lazily}");
    }
}

#[test]
#[should_panic(expected = "only a vertical ruler")]
fn a_horizontal_ruler_isn_t_provided_relatively() {
    let ruler = HorizontalRuler::new();
    let r = ruler.clone();
    let _host = host(move || {
        let r = r.clone();
        Leaf(Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let r = r.clone();
            scope.layout_with_rulers(
                5,
                5,
                move |rulers| rulers.provides_relative(&r, 1.0),
                move |placement| placement.place(&placeable, 0, 0),
            )
        }));
    });
}
