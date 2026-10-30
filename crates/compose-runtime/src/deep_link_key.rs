//! Decoding a value from the arguments of a deep link, as Navigation 3's `DeepLinkDecoder`: what
//! kotlinx.serialization's generated serializers do for it, `#[derive(DeepLinkKey)]` does for a
//! struct, field by field. Beside `Saveable`, as the derive's code goes through
//! `compose::__private`; `compose-navigation` has the matchers using it (D1414).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::hash::Hash;

/// Each argument's values, by name: a flat map, whatever the key's shape.
pub type DeepLinkArguments = HashMap<String, Vec<String>>;

/// Why a key couldn't be decoded: a matcher takes it for no match, as `DeepLinkDecoderException`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeepLinkDecoderError(pub String);

impl DeepLinkDecoderError {
    /// A required field with no value.
    pub fn missing(name: &str) -> DeepLinkDecoderError {
        DeepLinkDecoderError(format!("Missing argument for required field [{name}]"))
    }
}

impl fmt::Display for DeepLinkDecoderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DeepLinkDecoderError {}

/// A value a deep link's arguments can make, as a type with a serializer: `#[derive(DeepLinkKey)]`
/// for a struct, whose fields are read from the same flat map at any depth, and for an enum of unit
/// variants, by name. A map, a collection of what's not primitive and open polymorphism don't
/// implement it, which Kotlin's decoder finds out when it runs.
#[diagnostic::on_unimplemented(
    message = "`{Self}` can't be decoded from a deep link's arguments",
    label = "not a primitive, a collection of primitives or a `#[derive(DeepLinkKey)]` type",
    note = "a deep link's arguments are a flat map of strings: a map, a collection of keys and an \
            abstract type have no form in it, and a type of its own form reads with \
            `#[deep_link(with = serializer)]`"
)]
pub trait DeepLinkKey: Sized {
    /// Whether it's a key of fields, read from the same flat map: nullable, it's there by an
    /// argument of its own name, as the decoder's `decodeNotNullMark`.
    const IS_NESTED: bool = false;

    /// The value of the field `name` of what's being decoded: `None` if it has no value, which
    /// is its default if it has one, and otherwise no match. An error if it has one of the wrong
    /// type.
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError>;
}

/// A value of one string, which collections can hold, as a primitive's serializer.
pub trait DeepLinkPrimitive: Sized {
    /// Whether an empty string is a value: only for a string. Otherwise it's none.
    const EMPTY_IS_A_VALUE: bool = false;

    fn parse(value: &str) -> Result<Self, DeepLinkDecoderError>;
}

/// What a collection holds: a primitive, or one that may be null, written `null`.
pub trait DeepLinkElement: Sized {
    fn parse_element(value: &str) -> Result<Self, DeepLinkDecoderError>;
}

impl<T: DeepLinkPrimitive> DeepLinkElement for T {
    fn parse_element(value: &str) -> Result<Self, DeepLinkDecoderError> {
        T::parse(value)
    }
}

impl<T: DeepLinkPrimitive> DeepLinkElement for Option<T> {
    fn parse_element(value: &str) -> Result<Self, DeepLinkDecoderError> {
        if value == "null" { Ok(None) } else { T::parse(value).map(Some) }
    }
}

fn primitive_field<T: DeepLinkPrimitive>(
    arguments: &DeepLinkArguments,
    name: &str,
) -> Result<Option<T>, DeepLinkDecoderError> {
    let Some(value) = arguments.get(name).and_then(|values| values.first()) else {
        return Ok(None);
    };
    if value.is_empty() && !T::EMPTY_IS_A_VALUE {
        return Ok(None);
    }
    T::parse(value).map(Some)
}

macro_rules! parsed_primitives {
    ($($ty:ty),*) => {$(
        impl DeepLinkPrimitive for $ty {
            fn parse(value: &str) -> Result<Self, DeepLinkDecoderError> {
                value.parse().map_err(|error| {
                    DeepLinkDecoderError(format!("{value}: {error}"))
                })
            }
        }

        impl DeepLinkKey for $ty {
            fn decode_field(
                arguments: &DeepLinkArguments,
                name: &str,
            ) -> Result<Option<Self>, DeepLinkDecoderError> {
                primitive_field(arguments, name)
            }
        }
    )*};
}

parsed_primitives!(bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// `value` as Java's `parseFloat` and `parseDouble` read it, what Kotlin's `toFloat` and `toDouble`
/// are: blanks around it, a type's suffix, `NaN` and `Infinity` spelled so and no other way. Hex
/// floats aren't read (D1415).
fn java_float_text(value: &str) -> Result<&str, DeepLinkDecoderError> {
    let wrong = || DeepLinkDecoderError(format!("For input string: \"{value}\""));
    let trimmed = value.trim_matches(|character: char| character <= ' ');
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if unsigned == "NaN" || unsigned == "Infinity" {
        return Ok(trimmed);
    }
    if !unsigned.starts_with(|first: char| first.is_ascii_digit() || first == '.') {
        return Err(wrong());
    }
    Ok(trimmed.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(trimmed))
}

macro_rules! float_primitives {
    ($($ty:ty),*) => {$(
        impl DeepLinkPrimitive for $ty {
            fn parse(value: &str) -> Result<Self, DeepLinkDecoderError> {
                java_float_text(value)?
                    .parse()
                    .map_err(|error| DeepLinkDecoderError(format!("{value}: {error}")))
            }
        }

        impl DeepLinkKey for $ty {
            fn decode_field(
                arguments: &DeepLinkArguments,
                name: &str,
            ) -> Result<Option<Self>, DeepLinkDecoderError> {
                primitive_field(arguments, name)
            }
        }
    )*};
}

float_primitives!(f32, f64);

impl DeepLinkPrimitive for char {
    fn parse(value: &str) -> Result<Self, DeepLinkDecoderError> {
        value
            .chars()
            .next()
            .ok_or_else(|| DeepLinkDecoderError("An empty string isn't a character".to_string()))
    }
}

impl DeepLinkKey for char {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        primitive_field(arguments, name)
    }
}

impl DeepLinkPrimitive for String {
    const EMPTY_IS_A_VALUE: bool = true;

    fn parse(value: &str) -> Result<Self, DeepLinkDecoderError> {
        Ok(value.to_string())
    }
}

impl DeepLinkKey for String {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        primitive_field(arguments, name)
    }
}

/// A nullable field is required as any other, unless it's given a default; but for a nested key.
impl<T: DeepLinkKey> DeepLinkKey for Option<T> {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        if T::IS_NESTED && !arguments.contains_key(name) {
            return Ok(Some(None));
        }
        T::decode_field(arguments, name).map(|value| value.map(Some))
    }
}

/// Every value of an argument named by a collection, its repeats in order; an empty first value is
/// none, as of any field but a string.
fn collection_field<T: DeepLinkElement, C: FromIterator<T>>(
    arguments: &DeepLinkArguments,
    name: &str,
) -> Result<Option<C>, DeepLinkDecoderError> {
    let Some(values) = arguments.get(name) else { return Ok(None) };
    if values.first().is_none_or(String::is_empty) {
        return Ok(None);
    }
    values.iter().map(|value| T::parse_element(value)).collect::<Result<C, _>>().map(Some)
}

impl<T: DeepLinkElement> DeepLinkKey for Vec<T> {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        collection_field(arguments, name)
    }
}

impl<T: DeepLinkElement> DeepLinkKey for Box<[T]> {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        collection_field(arguments, name)
    }
}

impl<T: DeepLinkElement + Eq + Hash> DeepLinkKey for HashSet<T> {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        collection_field(arguments, name)
    }
}

impl<T: DeepLinkElement + Ord> DeepLinkKey for BTreeSet<T> {
    fn decode_field(
        arguments: &DeepLinkArguments,
        name: &str,
    ) -> Result<Option<Self>, DeepLinkDecoderError> {
        collection_field(arguments, name)
    }
}

/// A value going by a string of its own, as a `KSerializer` Kotlin's `DeepLinkSerializer` is: for
/// an argument of a type not otherwise decoded, named with `#[deep_link(with = Serializer)]`.
pub trait DeepLinkSerializer<T> {
    /// What identifies the type.
    fn serial_name(&self) -> &str;

    /// The value of `value`, which [`serialize`](Self::serialize) makes.
    fn deserialize(&self, value: &str) -> T;

    /// What [`deserialize`](Self::deserialize) reads.
    fn serialize(&self, value: &T) -> String;
}

/// The field `name` by `serializer`: a string, so an empty one is a value.
#[doc(hidden)]
pub fn decode_with<T>(
    serializer: &impl DeepLinkSerializer<T>,
    arguments: &DeepLinkArguments,
    name: &str,
) -> Option<T> {
    let value = arguments.get(name).and_then(|values| values.first())?;
    Some(serializer.deserialize(value))
}
