//! The `#[composable]` transform and `#[derive(ComposeArg)]` of compose-rs.
//!
//! The transform is syntactic (docs/03-composable-macro.md): it sees tokens, not types, so where
//! it can't know something it emits code that lets rustc decide through traits. Its output calls
//! shared, non-generic runtime functions through `::compose::__private`, so expansions stay small.
//! Crates that use it without the `compose` facade alias their runtime dependency:
//! `extern crate compose_runtime as compose;`.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;

mod call;
mod composable;
mod derive;
mod memo;
mod regions;

#[cfg(test)]
mod golden;

/// Makes a function a composable: a restartable, skippable group in the composition.
///
/// ```ignore
/// #[composable]
/// fn Greeting(name: SharedString, count: i32) {
///     Text(format!("Hello {name}: {count}"));
/// }
/// ```
///
/// - The function gets a restart group keyed on its declaration path. Its arguments are kept
///   and compared with `ComposeArg::same` on the next pass: when every argument is the same and
///   its scope isn't invalid, the body is skipped. Invalidated, it restarts on its own with the
///   kept arguments.
/// - Branches, match arms and loops containing likely composable calls get region groups, so
///   state doesn't jump between branches.
/// - Composables with borrowed parameters, or returning a value, are non-restartable: they run
///   inline whenever their caller runs. `#[composable(non_restartable)]` asks for it explicitly.
#[proc_macro_attribute]
pub fn composable(attr: TokenStream, item: TokenStream) -> TokenStream {
    // The attribute's position only names the exported call macro uniquely; keys don't use it.
    let span = proc_macro::Span::call_site();
    let site = format!("{}:{}", span.line(), span.column());
    composable::expand(attr.into(), item.into(), &declaration_scope(), &site).into()
}

/// The crate and source file of the composable being expanded: with its name, what its group
/// key hashes. It doesn't depend on line numbers or formatting, so keys stay stable across
/// unrelated edits, and the path is relative to the package so keys are the same on every
/// machine.
fn declaration_scope() -> String {
    let krate = std::env::var("CARGO_CRATE_NAME").unwrap_or_default();
    let file = std::path::PathBuf::from(proc_macro::Span::call_site().file());
    // Cargo passes a workspace member's sources relative to the workspace root (rustc's working
    // directory) and a dependency's as absolute paths: resolve, then make relative to the package.
    let file = std::env::current_dir().map(|cwd| cwd.join(&file)).unwrap_or(file);
    let relative = |root: Option<std::ffi::OsString>, prefix: &str| {
        let root = std::path::PathBuf::from(root?);
        let rest = file.strip_prefix(&root).ok()?;
        Some(std::path::Path::new(prefix).join(rest))
    };
    // Relative to the package; else to the build script's output; else (a `macro_rules!` of
    // another crate) only the file's name, so keys never hold a machine's absolute path.
    let file = relative(std::env::var_os("CARGO_MANIFEST_DIR"), "")
        .or_else(|| relative(std::env::var_os("OUT_DIR"), "OUT_DIR"))
        .unwrap_or_else(|| file.file_name().map_or_else(Default::default, Into::into));
    let file: Vec<String> =
        file.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    format!("{krate}::{}", file.join("/"))
}

/// The call macros' implementation: binds positional, named, default, trailing and format
/// arguments to a composable's parameters. Not called directly: `Name!(…)` expands to it.
#[doc(hidden)]
#[proc_macro]
pub fn call(input: TokenStream) -> TokenStream {
    call::expand(input.into()).into()
}

/// Derives `ComposeArg` field by field: `Rc` fields compare by identity, small fields by value.
/// Enums compare variant and fields. The Rust analogue of a stable data class. A field marked
/// `#[compose_arg(eq)]` is compared with `PartialEq` instead (a short `Vec<String>`).
/// `Saveable`, field by field: a struct is a list of its fields, an enum a list of its variant's
/// name and fields. A saved value of another shape restores to `None`, so a remembered value is
/// computed anew, and so does one from before a variant's fields changed.
#[proc_macro_derive(Saveable)]
pub fn derive_saveable(input: TokenStream) -> TokenStream {
    derive::expand_saveable(input.into()).into()
}

/// `DeepLinkKey`, as the serializer Kotlin generates for what a deep link decodes: a struct field by
/// field from the flat map of arguments, each by its name or `#[deep_link(rename = "name")]`, with
/// `#[deep_link(default)]` or `#[deep_link(default = value)]` for what may be absent and
/// `#[deep_link(with = serializer)]` to read it with a `DeepLinkSerializer`; an enum of unit
/// variants by the variant's name.
#[proc_macro_derive(DeepLinkKey, attributes(deep_link))]
pub fn derive_deep_link_key(input: TokenStream) -> TokenStream {
    derive::expand_deep_link_key(input.into()).into()
}

#[proc_macro_derive(ComposeArg, attributes(compose_arg))]
pub fn derive_compose_arg(input: TokenStream) -> TokenStream {
    derive::expand(input.into()).into()
}
