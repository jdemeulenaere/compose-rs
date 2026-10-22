//! `#[derive(ComposeArg)]`: field-wise `same`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields, parse_quote};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match derive(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// Field-wise `same`, or `PartialEq` for a field marked `#[compose_arg(eq)]`.
fn same(a: TokenStream, b: TokenStream, field: &syn::Field) -> syn::Result<TokenStream> {
    Ok(if compared_by_value(field)? {
        quote_spanned! { field.ty.span()=> ::core::cmp::PartialEq::eq(#a, #b) }
    } else {
        quote_spanned! { field.ty.span()=> ::compose::__private::ComposeArg::same(#a, #b) }
    })
}

/// Whether a field is marked `#[compose_arg(eq)]`: compared by `PartialEq`, as a collection the
/// model knows to be small (`#[compose_arg(eq)] tags: Vec<String>`).
fn compared_by_value(field: &syn::Field) -> syn::Result<bool> {
    let mut eq = false;
    for attr in field.attrs.iter().filter(|attr| attr.path().is_ident("compose_arg")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("eq") {
                eq = true;
                Ok(())
            } else {
                Err(meta.error("unknown `compose_arg` option; expected `eq`"))
            }
        })?;
    }
    Ok(eq)
}

fn all(conditions: Vec<TokenStream>) -> TokenStream {
    if conditions.is_empty() { quote!(true) } else { quote!(#(#conditions)&&*) }
}

/// Whether tokens mention one of the given identifiers.
fn mentions(tokens: TokenStream, names: &[String]) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => names.iter().any(|name| ident == name),
        proc_macro2::TokenTree::Group(group) => mentions(group.stream(), names),
        _ => false,
    })
}

fn derive(input: TokenStream) -> syn::Result<TokenStream> {
    let mut input: DeriveInput = syn::parse2(input)?;
    // Bounds on the fields that use a type parameter (`T`, `Option<T>`), not on the parameters:
    // `Rc<T>` is `ComposeArg` for any `T`, so `Shared<Vec<u32>>` works.
    let parameters: Vec<String> =
        input.generics.type_params().map(|param| param.ident.to_string()).collect();
    let fields: Vec<syn::Field> = match &input.data {
        Data::Struct(data) => data.fields.iter().cloned().collect(),
        Data::Enum(data) => {
            data.variants.iter().flat_map(|variant| variant.fields.iter().cloned()).collect()
        }
        Data::Union(_) => Vec::new(),
    };
    let mut bounds: Vec<syn::WherePredicate> = Vec::new();
    for field in fields {
        let ty = &field.ty;
        if mentions(quote!(#ty), &parameters) {
            bounds.push(if compared_by_value(&field)? {
                parse_quote!(#ty: ::core::cmp::PartialEq + 'static)
            } else {
                parse_quote!(#ty: ::compose::__private::ComposeArg)
            });
        }
    }
    if !bounds.is_empty() {
        input.generics.make_where_clause().predicates.extend(bounds);
    }
    // Fields of a packed struct can't be borrowed in place: compare copies (they must be `Copy`).
    let packed = input
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("repr") && quote!(#attr).to_string().contains("packed"));
    let body = match &input.data {
        Data::Struct(data) => {
            let conditions = data
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let member = match &field.ident {
                        Some(name) => quote!(#name),
                        None => {
                            let index = syn::Index::from(index);
                            quote!(#index)
                        }
                    };
                    if packed {
                        same(quote!(&{ self.#member }), quote!(&{ other.#member }), field)
                    } else {
                        same(quote!(&self.#member), quote!(&other.#member), field)
                    }
                })
                .collect::<syn::Result<_>>()?;
            all(conditions)
        }
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| -> syn::Result<TokenStream> {
                let name = &variant.ident;
                let left = |prefix: &str| -> Vec<syn::Ident> {
                    (0..variant.fields.len()).map(|index| format_ident!("__{}{}", prefix, index)).collect()
                };
                let (ours, theirs) = (left("self"), left("other"));
                let conditions: Vec<TokenStream> = variant
                    .fields
                    .iter()
                    .zip(ours.iter().zip(&theirs))
                    .map(|(field, (a, b))| same(quote!(#a), quote!(#b), field))
                    .collect::<syn::Result<_>>()?;
                let condition = all(conditions);
                Ok(match &variant.fields {
                    Fields::Named(fields) => {
                        let names: Vec<_> = fields.named.iter().map(|field| &field.ident).collect();
                        quote! {
                            (Self::#name { #(#names: #ours),* }, Self::#name { #(#names: #theirs),* }) => #condition,
                        }
                    }
                    Fields::Unnamed(_) => quote! {
                        (Self::#name(#(#ours),*), Self::#name(#(#theirs),*)) => #condition,
                    },
                    Fields::Unit => quote! { (Self::#name, Self::#name) => true, },
                })
            });
            let arms = arms.collect::<syn::Result<Vec<_>>>()?;
            let fallback = (data.variants.len() > 1).then(|| quote!(_ => false,));
            if data.variants.is_empty() {
                quote!(match *self {})
            } else {
                quote! {
                    match (self, other) {
                        #(#arms)*
                        #fallback
                    }
                }
            }
        }
        Data::Union(union) => {
            return Err(Error::new(
                union.union_token.span(),
                "ComposeArg can't be derived for a union",
            ));
        }
    };
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::compose::__private::ComposeArg for #name #type_generics #where_clause {
            #[inline]
            fn same(&self, other: &Self) -> bool {
                #body
            }
        }
    })
}
