//! `#[derive(Saveable)]`: a key of a back stack, or any value of the app's, saved by its fields.

use compose_runtime::*;

fn round_trip<T: Saveable + PartialEq + std::fmt::Debug>(value: T) {
    let bytes = {
        let mut state = SavedState::default();
        state.values.insert(1, vec![value.save()]);
        state.to_bytes()
    };
    let state = SavedState::from_bytes(&bytes).expect("saved state bytes");
    assert_eq!(T::restore(&state.values[&1][0]), Some(value));
}

#[derive(PartialEq, Debug, Saveable)]
struct Unit;

#[derive(PartialEq, Debug, Saveable)]
struct Tuple(i32, String);

#[derive(PartialEq, Debug, Saveable)]
struct Named {
    id: i64,
    title: Option<String>,
    nested: Tuple,
}

#[derive(PartialEq, Debug, Saveable)]
enum Screen {
    Home,
    Details(i32),
    Search { query: String, page: i32 },
    Nested(Box<Inner>),
}

#[derive(PartialEq, Debug, Saveable)]
struct Inner(Vec<bool>);

impl Saveable for Box<Inner> {
    fn save(&self) -> SavedValue {
        (**self).save()
    }
    fn restore(saved: &SavedValue) -> Option<Self> {
        Inner::restore(saved).map(Box::new)
    }
}

#[derive(PartialEq, Debug, Saveable)]
struct Generic<T>(T, Vec<T>);

#[test]
fn structs_come_back_by_their_fields() {
    round_trip(Unit);
    round_trip(Tuple(1, "a".to_string()));
    round_trip(Named { id: 3, title: None, nested: Tuple(2, "b".to_string()) });
    round_trip(Named { id: 3, title: Some("t".to_string()), nested: Tuple(2, String::new()) });
    round_trip(Generic(1, vec![2, 3]));
}

#[test]
fn enums_come_back_as_their_variant() {
    round_trip(Screen::Home);
    round_trip(Screen::Details(4));
    round_trip(Screen::Search { query: "q".to_string(), page: 2 });
    round_trip(Screen::Nested(Box::new(Inner(vec![true, false]))));
}

/// What was saved by another version of the app restores to nothing, for the value to be computed
/// anew.
#[test]
fn another_shape_restores_to_nothing() {
    let text = |name: &str| SavedValue::Text(name.to_string());
    assert_eq!(Screen::restore(&SavedValue::Int(1)), None);
    assert_eq!(Screen::restore(&SavedValue::List(vec![])), None);
    assert_eq!(Screen::restore(&SavedValue::List(vec![text("Gone")])), None);
    assert_eq!(Screen::restore(&SavedValue::List(vec![text("Details")])), None);
    assert_eq!(Screen::restore(&SavedValue::List(vec![text("Details"), text("x")])), None);
    assert_eq!(Tuple::restore(&SavedValue::List(vec![SavedValue::Int(1)])), None);
    assert_eq!(Tuple::restore(&text("x")), None);
}
