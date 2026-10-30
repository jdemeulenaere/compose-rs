//! Deep links' keys, matchers, requests and back stacks, against Navigation 3's
//! `DeepLinkDecoderTest`, `DeepLinkMatcherTest`, `DeepLinkRequestTest`, `DeepLinkSerializerTest`,
//! `WrappedMatchResultTest` and `WithBackStackTest`. What Kotlin's decoder finds out when it runs (a
//! map, a collection of what's not primitive, an abstract class) doesn't compile here.

use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use compose_navigation::*;
use compose_runtime::*;
use compose_test::*;

pub fn arguments<const N: usize>(entries: [(&str, &[&str]); N]) -> DeepLinkArguments {
    entries
        .into_iter()
        .map(|(name, values)| {
            (name.to_string(), values.iter().map(|value| value.to_string()).collect())
        })
        .collect()
}

fn decode<T: DeepLinkKey>(args: DeepLinkArguments) -> Result<T, DeepLinkDecoderError> {
    T::decode_field(&args, "")?.ok_or_else(|| DeepLinkDecoderError::missing(""))
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct SimpleKey {
    pub name: String,
    pub age: i32,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct DefaultKey {
    #[deep_link(default = "default".to_string())]
    pub name: String,
    #[deep_link(default)]
    pub age: i32,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct NullableDefaultKey {
    #[deep_link(default)]
    pub name: Option<String>,
    #[deep_link(default)]
    pub age: Option<i32>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct NullableKey {
    pub name: Option<String>,
    pub age: Option<i32>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct NestedKey {
    pub user: SimpleKey,
    pub flag: bool,
}

#[derive(DeepLinkKey, Clone, Copy, PartialEq, Debug)]
pub enum Direction {
    #[deep_link(rename = "NORTH")]
    North,
    #[deep_link(rename = "SOUTH")]
    South,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct EnumKey {
    pub direction: Direction,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct NestedEnumKey {
    direction: EnumKey,
    flag: bool,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct DefaultEnumKey {
    #[deep_link(default = Direction::North)]
    direction: Direction,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct ListKey {
    pub list: Vec<i32>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct SetKey {
    set: BTreeSet<String>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct ArrayKey {
    array: Box<[bool]>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct NullableListKey {
    list: Vec<Option<i32>>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct DefaultListKey {
    #[deep_link(default)]
    list: Vec<i32>,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct NonEmptyDefaultListKey {
    #[deep_link(default = vec![1, 2, 3])]
    list: Vec<i32>,
}

macro_rules! key_of_one {
    ($($key:ident { $field:ident: $ty:ty })*) => {$(
        #[derive(DeepLinkKey, Clone, PartialEq, Debug)]
        pub struct $key {
            pub $field: $ty,
        }
    )*};
}

key_of_one! {
    BooleanKey { bool: bool }
    ByteKey { byte: i8 }
    ShortKey { short: i16 }
    IntKey { int: i32 }
    LongKey { long: i64 }
    FloatKey { float: f32 }
    DoubleKey { double: f64 }
    CharKey { char: char }
    TestArgKey { name: String }
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct TestDefaultArgKey {
    #[deep_link(default = "test".to_string())]
    pub name: String,
}

#[derive(DeepLinkKey, Clone, Copy, PartialEq, Debug)]
pub struct TestKey;

/// A class of two fields from any base, as `SerializableConcrete`: a struct, flat in the arguments.
#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct ConcreteKey {
    pub owner: String,
    pub name: String,
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
pub struct ConcreteArgKey {
    pub derived: ConcreteKey,
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeSimpleKey
#[test]
fn a_key_is_decoded_field_by_field() {
    let key: SimpleKey = decode(arguments([("name", &["john"]), ("age", &["30"])])).unwrap();
    assert_eq!(key, SimpleKey { name: "john".into(), age: 30 });
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeMissingRequiredFieldThrows
#[test]
fn a_required_field_with_no_value_fails() {
    assert!(decode::<SimpleKey>(arguments([("name", &["john"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeDefaultValues
#[test]
fn absent_fields_take_their_defaults() {
    assert_eq!(
        decode::<DefaultKey>(HashMap::new()).unwrap(),
        DefaultKey { name: "default".into(), age: 0 }
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNullableValues_present
#[test]
fn a_nullable_field_takes_its_value() {
    let key: NullableKey = decode(arguments([("name", &["john"]), ("age", &["30"])])).unwrap();
    assert_eq!(key, NullableKey { name: Some("john".into()), age: Some(30) });
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNullableValues_missingThrows
#[test]
fn a_nullable_field_without_a_default_is_still_required() {
    assert!(decode::<NullableKey>(HashMap::new()).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNestedObject
#[test]
fn a_nested_key_reads_the_same_flat_arguments() {
    let key: NestedKey =
        decode(arguments([("name", &["john"]), ("age", &["30"]), ("flag", &["true"])])).unwrap();
    assert_eq!(key, NestedKey { user: SimpleKey { name: "john".into(), age: 30 }, flag: true });
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeBoolean
#[test]
fn a_boolean_is_decoded() {
    assert!(decode::<BooleanKey>(arguments([("bool", &["true"])])).unwrap().bool);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeByte
#[test]
fn a_byte_is_decoded() {
    assert_eq!(decode::<ByteKey>(arguments([("byte", &["1"])])).unwrap().byte, 1);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeShort
#[test]
fn a_short_is_decoded() {
    assert_eq!(decode::<ShortKey>(arguments([("short", &["2"])])).unwrap().short, 2);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInt
#[test]
fn an_int_is_decoded() {
    assert_eq!(decode::<IntKey>(arguments([("int", &["1"])])).unwrap().int, 1);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeLong
#[test]
fn a_long_is_decoded() {
    assert_eq!(decode::<LongKey>(arguments([("long", &["4"])])).unwrap().long, 4);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeFloat
#[test]
fn a_float_is_decoded() {
    assert_eq!(decode::<FloatKey>(arguments([("float", &["5.0"])])).unwrap().float, 5.0);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeDouble
#[test]
fn a_double_is_decoded() {
    assert_eq!(decode::<DoubleKey>(arguments([("double", &["6.0"])])).unwrap().double, 6.0);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeChar
#[test]
fn a_char_is_decoded() {
    assert_eq!(decode::<CharKey>(arguments([("char", &["a"])])).unwrap().char, 'a');
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidIntThrows
#[test]
fn an_int_of_something_else_fails() {
    assert!(decode::<IntKey>(arguments([("int", &["notAnInt"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidBooleanThrows
#[test]
fn a_boolean_of_something_else_fails() {
    assert!(decode::<BooleanKey>(arguments([("bool", &["notABoolean"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidByteThrows
#[test]
fn a_byte_of_something_else_fails() {
    assert!(decode::<ByteKey>(arguments([("byte", &["notAByte"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidShortThrows
#[test]
fn a_short_of_something_else_fails() {
    assert!(decode::<ShortKey>(arguments([("short", &["notAShort"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidLongThrows
#[test]
fn a_long_of_something_else_fails() {
    assert!(decode::<LongKey>(arguments([("long", &["notALong"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidFloatThrows
#[test]
fn a_float_of_something_else_fails() {
    assert!(decode::<FloatKey>(arguments([("float", &["notAFloat"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidDoubleThrows
#[test]
fn a_double_of_something_else_fails() {
    assert!(decode::<DoubleKey>(arguments([("double", &["notADouble"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEmptyCharThrows
#[test]
fn an_empty_char_fails() {
    assert!(decode::<CharKey>(arguments([("char", &[""])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEnum
#[test]
fn an_enum_is_decoded_by_its_name() {
    let key: EnumKey = decode(arguments([("direction", &["NORTH"])])).unwrap();
    assert_eq!(key.direction, Direction::North);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNestedEnum
#[test]
fn a_nested_enum_is_decoded() {
    let key: NestedEnumKey =
        decode(arguments([("direction", &["NORTH"]), ("flag", &["true"])])).unwrap();
    assert_eq!(
        key,
        NestedEnumKey { direction: EnumKey { direction: Direction::North }, flag: true }
    );
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeDefaultEnum
#[test]
fn an_absent_enum_takes_its_default() {
    let key: DefaultEnumKey = decode(HashMap::new()).unwrap();
    assert_eq!(key.direction, Direction::North);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeInvalidEnumThrows
#[test]
fn an_enum_of_another_name_fails() {
    assert!(decode::<EnumKey>(arguments([("direction", &["UP"])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeList
#[test]
fn a_list_takes_every_value() {
    let key: ListKey = decode(arguments([("list", &["1", "2", "3"])])).unwrap();
    assert_eq!(key.list, [1, 2, 3]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeSet
#[test]
fn a_set_takes_each_value_once() {
    let key: SetKey = decode(arguments([("set", &["a", "b", "a"])])).unwrap();
    assert_eq!(key.set, BTreeSet::from(["a".to_string(), "b".to_string()]));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeArray
#[test]
fn an_array_takes_every_value() {
    let key: ArrayKey = decode(arguments([("array", &["true", "false", "true"])])).unwrap();
    assert_eq!(&*key.array, [true, false, true]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNullableList
#[test]
fn a_list_of_nullables_reads_null() {
    let key: NullableListKey = decode(arguments([("list", &["1", "null", "3"])])).unwrap();
    assert_eq!(key.list, [Some(1), None, Some(3)]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEmptyList
#[test]
fn an_absent_list_takes_its_empty_default() {
    assert!(decode::<DefaultListKey>(HashMap::new()).unwrap().list.is_empty());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNonEmptyDefaultList
#[test]
fn an_absent_list_takes_its_default() {
    assert_eq!(decode::<NonEmptyDefaultListKey>(HashMap::new()).unwrap().list, [1, 2, 3]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEmptyString
#[test]
fn an_empty_string_is_a_string() {
    let key: SimpleKey = decode(arguments([("name", &[""]), ("age", &["30"])])).unwrap();
    assert_eq!(key, SimpleKey { name: String::new(), age: 30 });
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEmptyString_nullableIntFieldWithNullFallback
#[test]
fn an_empty_int_is_absent_taking_its_default() {
    let key: NullableDefaultKey = decode(arguments([("name", &["john"]), ("age", &[""])])).unwrap();
    assert_eq!(key, NullableDefaultKey { name: Some("john".into()), age: None });
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEmptyString_nullableIntFieldNoFallbackFails
#[test]
fn an_empty_nullable_int_without_a_default_fails() {
    assert!(decode::<NullableKey>(arguments([("name", &["john"]), ("age", &[""])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeEmptyString_nonNullableIntFails
#[test]
fn an_empty_int_fails() {
    assert!(decode::<SimpleKey>(arguments([("name", &["john"]), ("age", &[""])])).is_err());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodePolymorphic
#[test]
fn a_concrete_key_from_a_base_is_decoded() {
    let key: ConcreteKey =
        decode(arguments([("name", &["the name"]), ("owner", &["the owner"])])).unwrap();
    assert_eq!(key, ConcreteKey { owner: "the owner".into(), name: "the name".into() });
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkDecoderTest.kt, testDecodeNestedPolymorphic
#[test]
fn a_nested_concrete_key_from_a_base_is_decoded() {
    let key: ConcreteArgKey =
        decode(arguments([("name", &["the name"]), ("owner", &["the owner"])])).unwrap();
    assert_eq!(key.derived, ConcreteKey { owner: "the owner".into(), name: "the name".into() });
}

/// What the tests' requests carry, as `MATCHER_STRING_FILTER_KEY`.
#[derive(Clone, Copy)]
struct FilterKey;

impl RequestExtrasKey for FilterKey {
    type Value = String;
}

const FILTER_VALUE: &str = "filterString";

fn with_string_extra(uri: &str, extra: &str) -> DeepLinkRequest {
    DeepLinkRequest::new(
        Some(DeepLinkUri::new(uri)),
        request_extras(|scope| scope.put(&FilterKey, extra.to_string())),
    )
}

fn request() -> DeepLinkRequest {
    with_string_extra("https://example.com", FILTER_VALUE)
}

fn filter(wanted: &'static str) -> DeepLinkFilter {
    DeepLinkFilter::new(move |request| {
        request.extras().get(&FilterKey).is_some_and(|v| v == wanted)
    })
}

/// `TestDeepLinkMatcher`: its filters, and then a key.
fn matcher(filters: Vec<DeepLinkFilter>) -> StaticKeyDeepLinkMatcher<TestKey> {
    StaticKeyDeepLinkMatcher::new(TestKey, filters)
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_emptyFilters
#[test]
fn no_filters_match_anything() {
    assert_eq!(*matcher(vec![]).try_match(&request()).unwrap().key(), TestKey);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_filterPasses
#[test]
fn a_filter_passing_matches() {
    assert!(matcher(vec![filter(FILTER_VALUE)]).try_match(&request()).is_some());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_filterFails
#[test]
fn a_filter_failing_doesn_t_match() {
    assert!(matcher(vec![filter("wrongFilter")]).try_match(&request()).is_none());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_allFiltersPass
#[test]
fn all_filters_passing_match() {
    let filters = vec![filter(FILTER_VALUE), filter(FILTER_VALUE)];
    assert!(matcher(filters).try_match(&request()).is_some());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_someFiltersFail
#[test]
fn one_filter_failing_doesn_t_match() {
    let filters = vec![filter(FILTER_VALUE), filter("wrongFilter")];
    assert!(matcher(filters).try_match(&request()).is_none());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_defaultComparator
#[test]
fn two_plain_results_compare_equal() {
    let (first, second) = (MatchResult::new(1), MatchResult::new(2));
    assert_eq!(first.compare_to(&second), Ordering::Equal);
    assert_eq!(second.compare_to(&first), Ordering::Equal);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, match_mimeTypeFilter
#[test]
fn a_mime_type_filter_ignores_case() {
    let matcher = matcher(vec![mime_type_filter("image/test")]);
    let of = |mime: &str| {
        DeepLinkRequest::new(Some(DeepLinkUri::new("https://example.com")), mime_type_extra(mime))
    };
    assert!(matcher.try_match(&of("image/TEST")).is_some());
    assert!(matcher.try_match(&of("image/wrongMimeType")).is_none());
}

/// The keys of an app, Kotlin's `TestInterface` and its objects.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TestInterface {
    ImplA,
    ImplB,
    ImplC,
}

struct OfKey(TestInterface);

impl DeepLinkMatcher<TestInterface> for OfKey {
    type Result = MatchResult<TestInterface>;

    fn match_request(&self, _request: &DeepLinkRequest) -> Option<MatchResult<TestInterface>> {
        Some(MatchResult::new(self.0))
    }
}

struct OfImplA;

impl DeepLinkMatcher<TestInterface> for OfImplA {
    type Result = MatchResult<TestInterface>;

    fn match_request(&self, _request: &DeepLinkRequest) -> Option<MatchResult<TestInterface>> {
        Some(MatchResult::new(TestInterface::ImplA))
    }
}

type ListedMatcher = Box<dyn DeepLinkMatcher<TestInterface, Result = MatchResult<TestInterface>>>;

fn keys_of(matchers: Vec<ListedMatcher>) -> Vec<TestInterface> {
    let request = DeepLinkRequest::of("https://www.testuri.com");
    matchers.iter().map(|matcher| *matcher.try_match(&request).unwrap().key()).collect()
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, compare_matcherClassesTypedOnDerivedKeys
#[test]
fn matchers_of_different_types_of_one_key_go_in_a_list() {
    let matchers: Vec<ListedMatcher> =
        vec![Box::new(OfImplA), Box::new(OfKey(TestInterface::ImplB))];
    assert_eq!(keys_of(matchers), [TestInterface::ImplA, TestInterface::ImplB]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, compare_matcherClassesTypedOnBaseKeys
#[test]
fn static_and_custom_matchers_of_one_key_go_in_a_list() {
    let matchers: Vec<ListedMatcher> = vec![
        Box::new(StaticKeyDeepLinkMatcher::new(TestInterface::ImplA, vec![])),
        Box::new(OfKey(TestInterface::ImplB)),
    ];
    assert_eq!(keys_of(matchers), [TestInterface::ImplA, TestInterface::ImplB]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkMatcherTest.kt, compare_sameMatcherClassTypedOnBaseKey
#[test]
fn matchers_of_one_type_go_in_a_list() {
    let matchers: Vec<ListedMatcher> =
        vec![Box::new(OfKey(TestInterface::ImplA)), Box::new(OfKey(TestInterface::ImplB))];
    assert_eq!(keys_of(matchers), [TestInterface::ImplA, TestInterface::ImplB]);
}

macro_rules! extras_key {
    ($($name:ident: $value:ty),*) => {$(
        #[derive(Clone, Copy)]
        struct $name;

        impl RequestExtrasKey for $name {
            type Value = $value;
        }
    )*};
}

extras_key!(IntExtra: i32, StringExtra: String, BoolExtra: bool, OtherIntExtra: i32);

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testUri
#[test]
fn a_request_of_a_uri_has_no_extras() {
    let request = DeepLinkRequest::of("test.com/test");
    assert_eq!(request.uri().unwrap().to_string(), "test.com/test");
    assert!(request.extras().is_empty());
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testExtra
#[test]
fn extras_keep_values_by_key() {
    let extras = request_extras(|scope| {
        scope.put(&IntExtra, 42);
        scope.put(&StringExtra, "test".to_string());
    });
    let request = DeepLinkRequest::new(None, extras);
    assert!(request.extras().is_not_empty());
    assert_eq!(request.extras().size(), 2);
    assert_eq!(request.extras().get(&IntExtra), Some(&42));
    assert_eq!(request.extras().get(&StringExtra).map(String::as_str), Some("test"));
    assert_eq!(request.extras().get(&BoolExtra), None);
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testEmptyRequestFails
#[test]
#[should_panic(expected = "either a uri or extras")]
fn a_request_of_nothing_panics() {
    DeepLinkRequest::new(None, empty_request_extras());
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testExtraContains
#[test]
fn extras_say_what_they_contain() {
    let extras = request_extras(|scope| scope.put(&IntExtra, 42));
    assert!(extras.contains(&IntExtra));
    assert!(!extras.contains(&BoolExtra));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testExtraDsl
#[test]
fn a_request_of_extras_has_no_uri() {
    let request = DeepLinkRequest::new(None, request_extras(|scope| scope.put(&BoolExtra, true)));
    assert!(request.uri().is_none());
    assert_eq!(request.extras().get(&BoolExtra), Some(&true));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testExtraWrongKey
#[test]
fn another_key_of_one_type_has_no_value() {
    let request = DeepLinkRequest::new(None, request_extras(|scope| scope.put(&IntExtra, 1)));
    assert_eq!(request.extras().get(&OtherIntExtra), None);
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testExtrasPlus
#[test]
fn extras_added_go_over_those_there() {
    let first = request_extras(|scope| scope.put(&IntExtra, 1));
    let second = request_extras(|scope| {
        scope.put(&IntExtra, 2);
        scope.put(&StringExtra, "test".to_string());
    });
    let combined = first + second;
    assert_eq!(combined.size(), 2);
    assert_eq!(combined.get(&IntExtra), Some(&2));
    assert_eq!(combined.get(&StringExtra).map(String::as_str), Some("test"));
}

// Compose: navigation3/navigation3-runtime/src/commonTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkRequestTest.kt, testExtrasMinus
#[test]
fn extras_taken_away_go_by_key() {
    let extras = request_extras(|scope| {
        scope.put(&IntExtra, 1);
        scope.put(&StringExtra, "test".to_string());
    });
    let without = extras - request_extras(|scope| scope.put(&IntExtra, 1));
    assert_eq!(without.size(), 1);
    assert_eq!(without.get(&IntExtra), None);
    assert_eq!(without.get(&StringExtra).map(String::as_str), Some("test"));
}

/// Extras compare by their values, and a request prints what it has.
#[test]
fn extras_compare_and_print_their_values() {
    let of = |value| request_extras(|scope| scope.put(&IntExtra, value));
    assert_eq!(of(1), of(1));
    assert_ne!(of(1), of(2));
    assert_ne!(of(1), empty_request_extras());
    let printed = DeepLinkRequest::new(Some(DeepLinkUri::new("a.com")), of(1)).to_string();
    assert!(printed.starts_with("DeepLinkRequest{ uri=a.com extras=RequestExtras("), "{printed}");
    assert!(printed.ends_with("=1) }"), "{printed}");
}

#[derive(Clone, PartialEq, Debug, Saveable)]
struct TestPerson {
    name: String,
    age: i32,
}

struct TestPersonSerializer;

impl DeepLinkSerializer<TestPerson> for TestPersonSerializer {
    fn serial_name(&self) -> &str {
        "androidx.navigation3.runtime.deeplink.TestPerson"
    }

    fn serialize(&self, value: &TestPerson) -> String {
        format!("{}--{}", value.name, value.age)
    }

    fn deserialize(&self, value: &str) -> TestPerson {
        let (name, age) = value.split_once("--").unwrap();
        TestPerson { name: name.to_string(), age: age.parse().unwrap() }
    }
}

fn alice() -> TestPerson {
    TestPerson { name: "Alice".into(), age: 30 }
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkSerializerTest.kt, serialize
#[test]
fn a_serializer_writes_its_string() {
    assert_eq!(TestPersonSerializer.serialize(&alice()), "Alice--30");
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkSerializerTest.kt, deserialize
#[test]
fn a_serializer_reads_its_string() {
    assert_eq!(TestPersonSerializer.deserialize("Alice--30"), alice());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkSerializerTest.kt, rememberSerializable_restoreValue
#[test]
fn a_value_is_restored_by_its_serializer() {
    thread_local! { static PERSON: std::cell::RefCell<Option<TestPerson>> = const { std::cell::RefCell::new(None) }; }
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let person = remember_saveable_with(serializer_saver(TestPersonSerializer), alice);
        PERSON.set(Some(person));
    });
    assert_eq!(PERSON.take(), Some(alice()));
    tester.emulate_save_and_restore(&mut rule);
    assert_eq!(PERSON.take(), Some(alice()));
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/DeepLinkSerializerTest.kt, rememberSerializable_restoreMutableState
#[test]
fn a_state_is_restored_by_its_serializer() {
    thread_local! { static STATE: Cell<Option<MutableState<TestPerson>>> = const { Cell::new(None) }; }
    let mut rule = compose_test(|| {});
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut rule, || {
        let state = remember_saveable_with(serializer_saver(TestPersonSerializer), || {
            mutable_state_of(alice())
        });
        STATE.set(Some(state));
    });
    assert_eq!(STATE.get().unwrap().get(), alice());
    STATE.get().unwrap().set(TestPerson { name: "Bob".into(), age: 40 });
    tester.emulate_save_and_restore(&mut rule);
    assert_eq!(STATE.get().unwrap().get(), TestPerson { name: "Bob".into(), age: 40 });
}

#[derive(Clone, Copy)]
struct Scored {
    key: u8,
    score: i32,
}

impl DeepLinkMatchResult<u8> for Scored {
    fn key(&self) -> &u8 {
        &self.key
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<u8>) -> Ordering {
        let other = other.wrapped().unwrap_or(other);
        if let Some(other) = other.as_any().downcast_ref::<Scored>() {
            return self.score.cmp(&other.score);
        }
        if let Some(other) = other.as_any().downcast_ref::<Prioritized>() {
            return self.score.cmp(&other.priority);
        }
        Ordering::Equal
    }
}

struct Prioritized {
    key: u8,
    priority: i32,
}

impl DeepLinkMatchResult<u8> for Prioritized {
    fn key(&self) -> &u8 {
        &self.key
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<u8>) -> Ordering {
        let other = other.wrapped().unwrap_or(other);
        if let Some(other) = other.as_any().downcast_ref::<Prioritized>() {
            return self.priority.cmp(&other.priority);
        }
        if let Some(other) = other.as_any().downcast_ref::<Scored>() {
            return self.priority.cmp(&other.score);
        }
        Ordering::Equal
    }
}

fn wrapped(score: i32) -> WrappedMatchResult<u8> {
    WrappedMatchResult::new(Scored { key: 1, score })
}

fn scored(score: i32) -> Scored {
    Scored { key: 2, score }
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WrappedMatchResultTest.kt, compareTo_nonWrapped
#[test]
fn a_wrapped_result_ranks_as_what_it_wraps() {
    assert_eq!(wrapped(5).compare_to(&scored(3)), Ordering::Greater);
    assert_eq!(wrapped(5).compare_to(&scored(10)), Ordering::Less);
    assert_eq!(wrapped(5).compare_to(&scored(5)), Ordering::Equal);
    assert_eq!(*wrapped(5).key(), 1);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WrappedMatchResultTest.kt, compareTo_wrappedSameInnerType
#[test]
fn two_wrapped_results_rank_by_what_they_wrap() {
    assert_eq!(wrapped(10).compare_to(&wrapped(5)), Ordering::Greater);
    assert_eq!(wrapped(5).compare_to(&wrapped(10)), Ordering::Less);
    assert_eq!(wrapped(10).compare_to(&wrapped(10)), Ordering::Equal);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WrappedMatchResultTest.kt, compareTo_wrappedDifferentInnerTypes
#[test]
fn wrapped_results_of_other_types_rank_by_those_types() {
    let other = WrappedMatchResult::new(Prioritized { key: 2, priority: 5 });
    assert_eq!(wrapped(10).compare_to(&other), Ordering::Greater);
    assert_eq!(other.compare_to(&wrapped(10)), Ordering::Less);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WrappedMatchResultTest.kt, nonWrapped_compareTo
#[test]
fn a_result_ranks_against_a_wrapped_one_by_what_it_wraps() {
    assert_eq!(scored(3).compare_to(&wrapped(5)), Ordering::Less);
    assert_eq!(scored(10).compare_to(&wrapped(5)), Ordering::Greater);
    assert_eq!(scored(5).compare_to(&wrapped(5)), Ordering::Equal);
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stacked {
    Parent,
    Key,
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt, withBackStack
#[test]
fn a_match_builds_its_back_stack() {
    let matcher = StaticKeyDeepLinkMatcher::new(Stacked::Key, vec![])
        .with_back_stack(|result| vec![Stacked::Parent, *result.key()]);
    let result = matcher.try_match(&request()).unwrap();
    assert_eq!(*result.key(), Stacked::Key);
    assert_eq!(result.back_stack(), [Stacked::Parent, Stacked::Key]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt, withBackStack_wrongNestedFilter
#[test]
fn a_nested_filter_failing_builds_nothing() {
    let called = Rc::new(Cell::new(false));
    let seen = called.clone();
    let matcher = matcher(vec![filter("wrongFilter")]).with_back_stack(move |result| {
        seen.set(true);
        vec![*result.key()]
    });
    assert!(matcher.try_match(&request()).is_none());
    assert!(!called.get());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt, withBackStack_emptyBackStack
#[test]
fn a_back_stack_can_be_empty() {
    let matcher = matcher(vec![]).with_back_stack(|_| Vec::<TestKey>::new());
    let result = matcher.try_match(&request()).unwrap();
    assert_eq!(*result.key(), TestKey);
    assert!(result.back_stack().is_empty());
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt, withBackStack_backStackAndKeyTypePreserved
#[test]
fn back_stack_matchers_of_one_key_go_in_a_list() {
    let of = |key| {
        StaticKeyDeepLinkMatcher::new(key, vec![])
            .with_back_stack(|result| vec![TestInterface::ImplC, *result.key()])
    };
    let matchers = [of(TestInterface::ImplA), of(TestInterface::ImplB)];
    let request = DeepLinkRequest::of("https://www.test.com");
    let first = matchers[0].try_match(&request).unwrap();
    assert_eq!(*first.key(), TestInterface::ImplA);
    assert_eq!(first.back_stack(), [TestInterface::ImplC, TestInterface::ImplA]);
}

// Compose: navigation3/navigation3-runtime/src/androidDeviceTest/kotlin/androidx/navigation3/runtime/deeplink/WithBackStackTest.kt, compareTo_basedOnNestedMatchResult
#[test]
fn a_back_stack_result_ranks_as_what_matched() {
    let of = |score| BackStackMatchResult::new(scored(score), |_| vec![0u8]);
    assert_eq!(of(10).compare_to(&of(5)), Ordering::Greater);
    assert_eq!(of(5).compare_to(&of(10)), Ordering::Less);
    assert_eq!(of(10).compare_to(&of(10)), Ordering::Equal);
}

/// It's built once, when first asked for.
#[test]
fn a_back_stack_is_built_once_and_when_asked() {
    let builds = Rc::new(Cell::new(0));
    let counted = builds.clone();
    let matcher = matcher(vec![]).with_back_stack(move |result| {
        counted.set(counted.get() + 1);
        vec![*result.key()]
    });
    let result = matcher.try_match(&request()).unwrap();
    assert_eq!(builds.get(), 0);
    let _ = result.back_stack();
    let _ = result.back_stack();
    assert_eq!(builds.get(), 1);
}

/// A collection's first value empty is no value, not one to read.
#[test]
fn an_empty_first_value_of_a_list_takes_its_default() {
    let key: DefaultListKey = decode(arguments([("list", &[""])])).unwrap();
    assert!(key.list.is_empty());
    assert!(decode::<ListKey>(arguments([("list", &[""])])).is_err());
}

/// Ranks only against its own type, so a wrapper has to look inside the other for it.
struct Plain(i32);

impl DeepLinkMatchResult<u8> for Plain {
    fn key(&self) -> &u8 {
        &0
    }

    fn compare_to(&self, other: &dyn DeepLinkMatchResult<u8>) -> Ordering {
        other.as_any().downcast_ref::<Plain>().map_or(Ordering::Equal, |other| self.0.cmp(&other.0))
    }
}

/// A wrapper gives what it wraps the other's inside, which it needn't know of.
#[test]
fn wrappers_compare_what_s_inside_each_other() {
    let wrapped = |score| WrappedMatchResult::new(Plain(score));
    assert_eq!(wrapped(5).compare_to(&wrapped(3)), Ordering::Greater);
    let stacked = |score| BackStackMatchResult::new(Plain(score), |_| vec![0u8]);
    assert_eq!(stacked(5).compare_to(&stacked(3)), Ordering::Greater);
    assert_eq!(stacked(5).compare_to(&wrapped(7)), Ordering::Less);
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct NullableNestedKey {
    user: Option<SimpleKey>,
    flag: bool,
}

/// A nullable nested key is there by an argument of its name: without it, it's null, whatever its
/// fields' arguments, as the decoder's `decodeNotNullMark`.
#[test]
fn a_nullable_nested_key_is_null_without_its_name() {
    let key: NullableNestedKey =
        decode(arguments([("name", &["john"]), ("age", &["30"]), ("flag", &["true"])])).unwrap();
    assert_eq!(key, NullableNestedKey { user: None, flag: true });
    let key: NullableNestedKey = decode(arguments([
        ("user", &["any"]),
        ("name", &["john"]),
        ("age", &["30"]),
        ("flag", &["true"]),
    ]))
    .unwrap();
    assert_eq!(key.user, Some(SimpleKey { name: "john".into(), age: 30 }));
}

/// As Kotlin's `toFloat` and `toDouble`, Java's: a type's suffix and its own spellings.
#[test]
fn floats_are_read_as_kotlin_reads_them() {
    let float = |value: &str| decode::<FloatKey>(arguments([("float", &[value])])).map(|k| k.float);
    let double =
        |value: &str| decode::<DoubleKey>(arguments([("double", &[value])])).map(|k| k.double);
    assert_eq!(float("1.5f"), Ok(1.5));
    assert_eq!(float("1.5F"), Ok(1.5));
    assert_eq!(float("2d"), Ok(2.0));
    assert_eq!(double("2.5D"), Ok(2.5));
    assert_eq!(double("2.5f"), Ok(2.5));
    assert_eq!(float(" 1.5 "), Ok(1.5));
    assert_eq!(float("-Infinity"), Ok(f32::NEG_INFINITY));
    assert_eq!(double("+Infinity"), Ok(f64::INFINITY));
    assert!(float("NaN").unwrap().is_nan());
    for wrong in ["inf", "Inf", "-inf", "nan", "infinity", "INFINITY", "Infinityf", "NaNf", "f", ""]
    {
        if !wrong.is_empty() {
            assert!(float(wrong).is_err(), "{wrong}");
            assert!(double(wrong).is_err(), "{wrong}");
        }
    }
}

key_of_one! {
    UsizeKey { value: usize }
    IsizeKey { value: isize }
    U128Key { value: u128 }
    I128Key { value: i128 }
}

/// Every integer of Rust's is a primitive.
#[test]
fn every_integer_is_a_primitive() {
    let of = |value: &'static [&'static str]| arguments([("value", value)]);
    assert_eq!(decode::<UsizeKey>(of(&["7"])).unwrap().value, 7);
    assert_eq!(decode::<IsizeKey>(of(&["-7"])).unwrap().value, -7);
    assert_eq!(
        decode::<U128Key>(of(&["340282366920938463463374607431768211455"])).unwrap().value,
        u128::MAX
    );
    assert_eq!(decode::<I128Key>(of(&["-7"])).unwrap().value, -7);
    assert!(decode::<UsizeKey>(of(&["-7"])).is_err());
}

#[derive(DeepLinkKey, Clone, PartialEq, Debug)]
struct OfAny<T> {
    value: T,
    #[deep_link(default)]
    others: Vec<i32>,
}

/// A key's type parameters are bound by what it needs of them.
#[test]
fn a_generic_key_is_decoded() {
    let key: OfAny<i32> = decode(arguments([("value", &["7"])])).unwrap();
    assert_eq!(key, OfAny { value: 7, others: vec![] });
    let key: OfAny<String> =
        decode(arguments([("value", &["a"]), ("others", &["1", "2"])])).unwrap();
    assert_eq!(key, OfAny { value: "a".to_string(), others: vec![1, 2] });
}
