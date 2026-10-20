//! Golden tests of the expansions: `tests/expand/<name>.rs` expands to
//! `tests/expand/<name>.expanded.rs`. Set `UPDATE_GOLDENS=1` to rewrite the expected files.

use std::fs;
use std::path::Path;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{Item, parse_quote};

/// Expands every `#[composable]` function and `#[derive(ComposeArg)]` item of a file.
fn expand_file(source: &str) -> String {
    let file: syn::File = syn::parse_str(source).expect("a valid input file");
    let mut items: Vec<Item> = Vec::new();
    for mut item in file.items {
        match &mut item {
            Item::Fn(function) => {
                if let Some(position) =
                    function.attrs.iter().position(|attr| attr.path().is_ident("composable"))
                {
                    let attr = function.attrs.remove(position);
                    let arguments = match attr.meta {
                        syn::Meta::List(list) => list.tokens,
                        _ => TokenStream::new(),
                    };
                    let expanded = crate::composable::expand(
                        arguments,
                        function.to_token_stream(),
                        "golden::src/lib.rs",
                    );
                    let expanded: syn::File = syn::parse2(expanded).expect("expansion parses");
                    items.extend(expanded.items);
                    continue;
                }
            }
            Item::Struct(_) | Item::Enum(_) => {
                let attrs = match &mut item {
                    Item::Struct(item) => &mut item.attrs,
                    Item::Enum(item) => &mut item.attrs,
                    _ => unreachable!(),
                };
                let derives = attrs.iter().any(|attr| {
                    attr.path().is_ident("derive")
                        && attr.to_token_stream().to_string().contains("ComposeArg")
                });
                if derives {
                    let expanded = crate::derive::expand(item.to_token_stream());
                    items.push(item);
                    let expanded: syn::File = syn::parse2(expanded).expect("derive parses");
                    items.extend(expanded.items);
                    continue;
                }
            }
            _ => {}
        }
        items.push(item);
    }
    let file: syn::File = parse_quote!(#(#items)*);
    prettyplease::unparse(&file)
}

#[test]
fn expansions_match_their_goldens() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expand");
    let update = std::env::var_os("UPDATE_GOLDENS").is_some();
    let mut checked = 0;
    let mut failures = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&directory).expect("tests/expand").flatten().collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
        if !name.ends_with(".rs") || name.ends_with(".expanded.rs") {
            continue;
        }
        let actual = expand_file(&fs::read_to_string(&path).expect("input"));
        let expected_path = path.with_extension("expanded.rs");
        if update {
            fs::write(&expected_path, &actual).expect("write golden");
        } else {
            let expected = fs::read_to_string(&expected_path).unwrap_or_default();
            if expected != actual {
                failures.push(format!("--- {name}: expected\n{expected}\n--- actual\n{actual}"));
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no golden inputs found");
    assert!(
        failures.is_empty(),
        "expansions differ (UPDATE_GOLDENS=1 to accept):\n{}",
        failures.join("\n")
    );
}
