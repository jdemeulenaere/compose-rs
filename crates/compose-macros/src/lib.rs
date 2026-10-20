//! The `#[composable]` transform and `#[derive(ComposeArg)]` of compose-rs.
//!
//! The transform is syntactic (docs/03-composable-macro.md): it sees tokens, not types, so where
//! it can't know something it emits code that lets rustc decide through traits. Its output calls
//! shared, non-generic runtime functions through `::compose::__private`, so expansions stay small.
//! Crates that use it without the `compose` facade alias their runtime dependency:
//! `extern crate compose_runtime as compose;`.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;

mod composable;
mod derive;
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
///   and compared with [`ComposeArg::same`] on the next pass: when every argument is the same and
///   its scope isn't invalid, the body is skipped. Invalidated, it restarts on its own with the
///   kept arguments.
/// - Branches, match arms and loops containing likely composable calls get region groups, so
///   state doesn't jump between branches.
/// - Composables with borrowed parameters, or returning a value, are non-restartable: they run
///   inline whenever their caller runs. `#[composable(non_restartable)]` asks for it explicitly.
#[proc_macro_attribute]
pub fn composable(attr: TokenStream, item: TokenStream) -> TokenStream {
    composable::expand(attr.into(), item.into(), &declaration_scope()).into()
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
    let file = match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(manifest) => file.strip_prefix(manifest).map(Into::into).unwrap_or(file),
        None => file,
    };
    let file: Vec<String> =
        file.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    format!("{krate}::{}", file.join("/"))
}

/// Derives `ComposeArg` field by field: `Rc` fields compare by identity, small fields by value.
/// Enums compare variant and fields. The Rust analogue of a stable data class.
#[proc_macro_derive(ComposeArg)]
pub fn derive_compose_arg(input: TokenStream) -> TokenStream {
    derive::expand(input.into()).into()
}
