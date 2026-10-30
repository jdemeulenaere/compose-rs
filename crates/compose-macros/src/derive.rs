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

pub(crate) fn expand_saveable(input: TokenStream) -> TokenStream {
    match derive_saveable(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// How `fields` are named in a pattern and a constructor, and what saves and restores them from
/// `__items` starting at `first`.
fn saveable_fields(
    fields: &Fields,
    first: usize,
) -> (Vec<TokenStream>, Vec<TokenStream>, Vec<TokenStream>) {
    let (mut pattern, mut saves, mut restores) = (Vec::new(), Vec::new(), Vec::new());
    for (index, field) in fields.iter().enumerate() {
        let binding = format_ident!("__field_{}", index);
        let at = first + index;
        let restore = quote! {
            <_ as ::compose::__private::Saveable>::restore(__items.get(#at)?)?
        };
        saves.push(quote! { ::compose::__private::Saveable::save(#binding) });
        match &field.ident {
            Some(name) => {
                pattern.push(quote! { #name: #binding });
                restores.push(quote! { #name: #restore });
            }
            None => {
                pattern.push(quote! { #binding });
                restores.push(restore);
            }
        }
    }
    (pattern, saves, restores)
}

/// `Name { … }`, `Name( … )` or `Name`, of what's given for its fields.
fn shaped(fields: &Fields, path: TokenStream, parts: &[TokenStream]) -> TokenStream {
    match fields {
        Fields::Named(_) => quote! { #path { #(#parts),* } },
        Fields::Unnamed(_) => quote! { #path ( #(#parts),* ) },
        Fields::Unit => path,
    }
}

fn derive_saveable(input: TokenStream) -> syn::Result<TokenStream> {
    let mut input: DeriveInput = syn::parse2(input)?;
    let parameters: Vec<String> =
        input.generics.type_params().map(|param| param.ident.to_string()).collect();
    let fields: Vec<syn::Field> = match &input.data {
        Data::Struct(data) => data.fields.iter().cloned().collect(),
        Data::Enum(data) => {
            data.variants.iter().flat_map(|variant| variant.fields.iter().cloned()).collect()
        }
        Data::Union(data) => {
            return Err(Error::new(data.union_token.span(), "a union can't derive `Saveable`"));
        }
    };
    for field in fields {
        let ty = &field.ty;
        if mentions(quote!(#ty), &parameters) {
            let predicate: syn::WherePredicate = parse_quote!(#ty: ::compose::__private::Saveable);
            input.generics.make_where_clause().predicates.push(predicate);
        }
    }
    let name = &input.ident;
    let (save, restore) = match &input.data {
        Data::Struct(data) => {
            let (pattern, saves, restores) = saveable_fields(&data.fields, 0);
            let binding = shaped(&data.fields, quote!(#name), &pattern);
            let made = shaped(&data.fields, quote!(#name), &restores);
            (
                quote! {
                    let #binding = self;
                    ::compose::__private::SavedValue::List(::std::vec![#(#saves),*])
                },
                quote! { ::core::option::Option::Some(#made) },
            )
        }
        Data::Enum(data) => {
            let (mut save_arms, mut restore_arms) = (Vec::new(), Vec::new());
            for variant in &data.variants {
                let variant_name = &variant.ident;
                let text = variant_name.to_string();
                let (pattern, saves, restores) = saveable_fields(&variant.fields, 1);
                let binding = shaped(&variant.fields, quote!(#name::#variant_name), &pattern);
                let made = shaped(&variant.fields, quote!(#name::#variant_name), &restores);
                save_arms.push(quote! {
                    #binding => ::compose::__private::SavedValue::List(::std::vec![
                        ::compose::__private::SavedValue::Text(::std::string::String::from(#text)),
                        #(#saves),*
                    ]),
                });
                restore_arms.push(quote! { #text => ::core::option::Option::Some(#made), });
            }
            (
                quote! {
                    match self {
                        #(#save_arms)*
                    }
                },
                quote! {
                    let ::compose::__private::SavedValue::Text(__name) = __items.first()? else {
                        return ::core::option::Option::None;
                    };
                    match __name.as_str() {
                        #(#restore_arms)*
                        _ => ::core::option::Option::None,
                    }
                },
            )
        }
        Data::Union(_) => unreachable!("refused above"),
    };
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::compose::__private::Saveable for #name #ty_generics #where_clause {
            fn save(&self) -> ::compose::__private::SavedValue {
                #save
            }

            fn restore(
                __saved: &::compose::__private::SavedValue,
            ) -> ::core::option::Option<Self> {
                let ::compose::__private::SavedValue::List(__items) = __saved else {
                    return ::core::option::Option::None;
                };
                #restore
            }
        }
    })
}
