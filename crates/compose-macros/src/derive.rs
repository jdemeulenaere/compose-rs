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

fn same(a: TokenStream, b: TokenStream, field: &syn::Field) -> TokenStream {
    quote_spanned! { field.ty.span()=> ::compose::__private::ComposeArg::same(#a, #b) }
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
    let fields: Vec<syn::Type> = match &input.data {
        Data::Struct(data) => data.fields.iter().map(|field| field.ty.clone()).collect(),
        Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter().map(|field| field.ty.clone()))
            .collect(),
        Data::Union(_) => Vec::new(),
    };
    let generic_fields: Vec<syn::Type> =
        fields.into_iter().filter(|ty| mentions(quote!(#ty), &parameters)).collect();
    if !generic_fields.is_empty() {
        let clause = input.generics.make_where_clause();
        for ty in generic_fields {
            clause.predicates.push(parse_quote!(#ty: ::compose::__private::ComposeArg));
        }
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
                .collect();
            all(conditions)
        }
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
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
                    .collect();
                let condition = all(conditions);
                match &variant.fields {
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
                }
            });
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
