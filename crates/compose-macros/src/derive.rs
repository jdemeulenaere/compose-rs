//! `#[derive(ComposeArg)]`: field-wise `same`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields, GenericParam, parse_quote};

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

fn derive(input: TokenStream) -> syn::Result<TokenStream> {
    let mut input: DeriveInput = syn::parse2(input)?;
    for param in &mut input.generics.params {
        if let GenericParam::Type(param) = param {
            param.bounds.push(parse_quote!(::compose::__private::ComposeArg));
        }
    }
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
                    same(quote!(&self.#member), quote!(&other.#member), field)
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
