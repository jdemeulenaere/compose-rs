//! `#[composable]` on a function definition.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Error, FnArg, GenericParam, Ident, ItemFn, Pat, PatIdent, ReturnType, Stmt, Type};

use crate::regions::Regions;

/// The 64-bit FNV-1a hash, as the runtime's `key_of`.
pub(crate) fn key_of(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream, scope: &str) -> TokenStream {
    match transform(attr, item.clone(), scope) {
        Ok(tokens) => tokens,
        Err(error) => {
            // Emit the function untransformed too, so the error doesn't cascade into
            // "cannot find function" errors at every call site.
            let mut tokens = error.to_compile_error();
            tokens.extend(item);
            tokens
        }
    }
}

#[derive(Default)]
struct Options {
    non_restartable: bool,
}

fn parse_options(attr: TokenStream) -> syn::Result<Options> {
    let mut options = Options::default();
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("non_restartable") {
            options.non_restartable = true;
            Ok(())
        } else {
            Err(meta.error("unknown #[composable] option; expected `non_restartable`"))
        }
    });
    syn::parse::Parser::parse2(parser, attr)?;
    Ok(options)
}

/// What a parameter's type contains, syntactically.
#[derive(Default)]
struct TypeScan {
    reference: bool,
    mutable_reference: Option<Span>,
    lifetime: bool,
    impl_trait: Option<Span>,
    fn_trait: bool,
}

impl<'ast> Visit<'ast> for TypeScan {
    fn visit_type_reference(&mut self, reference: &'ast syn::TypeReference) {
        // `&'static T` can be kept across passes; any other reference is a borrow.
        let is_static =
            reference.lifetime.as_ref().is_some_and(|lifetime| lifetime.ident == "static");
        if !is_static {
            self.reference = true;
        }
        if reference.mutability.is_some() {
            self.mutable_reference.get_or_insert(reference.span());
        }
        visit::visit_type_reference(self, reference);
    }

    fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
        if lifetime.ident != "static" {
            self.lifetime = true;
        }
    }

    fn visit_type_impl_trait(&mut self, impl_trait: &'ast syn::TypeImplTrait) {
        self.impl_trait.get_or_insert(impl_trait.span());
        visit::visit_type_impl_trait(self, impl_trait);
    }

    fn visit_path_segment(&mut self, segment: &'ast syn::PathSegment) {
        if segment.ident == "Fn" || segment.ident == "FnMut" || segment.ident == "FnOnce" {
            self.fn_trait = true;
        }
        // `Fn(&str) -> &str`: references in a function signature are higher-ranked, not a
        // borrow the argument holds.
        if !matches!(segment.arguments, syn::PathArguments::Parenthesized(_)) {
            visit::visit_path_segment(self, segment);
        }
    }

    fn visit_type_bare_fn(&mut self, _: &'ast syn::TypeBareFn) {
        // `fn(&str)`: as above. A function pointer is callable.
        self.fn_trait = true;
    }

    fn visit_bound_lifetimes(&mut self, _: &'ast syn::BoundLifetimes) {
        // `for<'a> …`: higher-ranked lifetimes.
    }
}

fn scan(ty: &Type) -> TypeScan {
    let mut scan = TypeScan::default();
    scan.visit_type(ty);
    scan
}

struct Param {
    name: Ident,
    ty: Type,
    /// The parameter's `#[cfg]`s, which its rebinding needs too.
    cfgs: Vec<syn::Attribute>,
    /// A parameter written as a pattern (`(x, y): (i32, i32)`) is renamed, and its pattern
    /// bound at the start of the body.
    pattern: Option<Pat>,
}

/// The generic type parameters bounded by `Fn`, `FnMut` or `FnOnce`.
fn generic_function_types(generics: &syn::Generics) -> Vec<Ident> {
    let bounded = |bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>| {
        bounds.iter().any(|bound| {
            matches!(bound, syn::TypeParamBound::Trait(bound)
                if bound.path.segments.last().is_some_and(|segment|
                    segment.ident == "Fn" || segment.ident == "FnMut" || segment.ident == "FnOnce"))
        })
    };
    let mut names: Vec<Ident> = generics
        .type_params()
        .filter(|param| bounded(&param.bounds))
        .map(|param| param.ident.clone())
        .collect();
    for predicate in generics.where_clause.iter().flat_map(|clause| &clause.predicates) {
        if let syn::WherePredicate::Type(predicate) = predicate
            && let Type::Path(path) = &predicate.bounded_ty
            && let Some(ident) = path.path.get_ident()
            && bounded(&predicate.bounds)
        {
            names.push(ident.clone());
        }
    }
    names
}

fn is_unit(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn transform(attr: TokenStream, item: TokenStream, scope: &str) -> syn::Result<TokenStream> {
    let options = parse_options(attr)?;
    let mut function: ItemFn = syn::parse2(item)?;
    let sig = &mut function.sig;
    if let Some(unsafety) = sig.unsafety {
        return Err(Error::new(
            unsafety.span(),
            "a composable can't be `unsafe`: the runtime calls it again from safe code when it \
             restarts; make it safe and put the `unsafe` block inside",
        ));
    }
    if let Some(constness) = sig.constness {
        return Err(Error::new(
            constness.span(),
            "a composable can't be `const`: it runs in a composition",
        ));
    }
    if let Some(asyncness) = sig.asyncness {
        return Err(Error::new(
            asyncness.span(),
            "a composable can't be `async`: composition is synchronous; run asynchronous work in \
             an effect",
        ));
    }

    let mut params = Vec::new();
    let mut borrowed =
        sig.generics.params.iter().any(|param| matches!(param, GenericParam::Lifetime(_)));
    let mut impl_trait = None;
    let mut conditional = false;
    for (index, input) in sig.inputs.iter_mut().enumerate() {
        let typed = match input {
            FnArg::Receiver(receiver) => {
                return Err(Error::new(
                    receiver.span(),
                    "a composable is a free function: methods can't be composables",
                ));
            }
            FnArg::Typed(typed) => typed,
        };
        let types = scan(&typed.ty);
        if let Some(span) = types.mutable_reference {
            return Err(Error::new(
                span,
                "a composable can't take `&mut`: it can't hold a mutable borrow across passes; \
                 pass a state handle (`MutableState`) instead",
            ));
        }
        borrowed |= types.reference || types.lifetime;
        // A `#[cfg]`-ed parameter may not exist: it can't be kept, so the composable runs inline.
        conditional |= typed.attrs.iter().any(|attr| attr.path().is_ident("cfg"));
        if impl_trait.is_none() {
            impl_trait = types.impl_trait;
        }
        let (name, pattern) = match &*typed.pat {
            Pat::Ident(PatIdent { by_ref: None, subpat: None, ident, .. }) => (ident.clone(), None),
            pattern => {
                let name = format_ident!("__compose_argument{}", index);
                let pattern = pattern.clone();
                // A `ref mut` binding in the pattern borrows the renamed argument mutably.
                let mutability = quote!(#pattern)
                    .to_string()
                    .contains("ref mut")
                    .then(<syn::Token![mut]>::default);
                *typed.pat = Pat::Ident(PatIdent {
                    attrs: Vec::new(),
                    by_ref: None,
                    mutability,
                    ident: name.clone(),
                    subpat: None,
                });
                (name, Some(pattern))
            }
        };
        let cfgs = typed.attrs.iter().filter(|attr| attr.path().is_ident("cfg")).cloned().collect();
        params.push(Param { name, ty: (*typed.ty).clone(), cfgs, pattern });
    }
    let returns_value = match &sig.output {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => !is_unit(ty),
    };
    let restartable = !options.non_restartable && !borrowed && !returns_value && !conditional;
    if restartable && let Some(span) = impl_trait {
        return Err(Error::new(
            span,
            "a restartable composable keeps its arguments, and an `impl Trait` argument \
                 can't be kept; use a concrete type or `Rc<dyn Trait>`, or mark the composable \
                 `#[composable(non_restartable)]`",
        ));
    }

    // Parameters called as composable lambdas: function-typed, or of a generic type bounded by a
    // function trait.
    let generic_functions = generic_function_types(&function.sig.generics);
    let callables: Vec<Ident> = params
        .iter()
        .filter(|param| {
            scan(&param.ty).fn_trait
                || matches!(&param.ty, Type::Path(path) if path.qself.is_none()
                    && path.path.get_ident().is_some_and(|ident| generic_functions.contains(ident)))
        })
        .map(|param| param.name.clone())
        .collect();
    // The group key: a hash of the crate, the file and the declaration, computed here so the
    // expansion holds a literal rather than a `const` item (each costs the compiler a body to
    // check and evaluate).
    let key = key_of(&format!("{scope}::{}", function.sig.ident));
    let mut regions = Regions::new(callables, key);
    regions.transform_block(&mut function.block);

    let rebinds = params.iter().filter_map(|param| {
        let pattern = param.pattern.as_ref()?;
        let name = &param.name;
        let cfgs = &param.cfgs;
        Some(quote! { #(#cfgs)* let #pattern = #name; })
    });
    let stmts: &Vec<Stmt> = &function.block.stmts;
    let name = &function.sig.ident;
    let key = proc_macro2::Literal::u64_suffixed(key);
    let group = Ident::new("__compose_group", Span::mixed_site());

    let body = if restartable {
        let composer = Ident::new("__composer", Span::mixed_site());
        let kept = Ident::new("__kept", Span::mixed_site());
        let types: Vec<&Type> = params.iter().map(|param| &param.ty).collect();
        let (_, type_generics, _) = function.sig.generics.split_for_impl();
        let turbofish = type_generics.as_turbofish();
        let restart = if params.is_empty() {
            quote! { |_: &dyn ::core::any::Any| #name #turbofish() }
        } else {
            let arguments = params.iter().enumerate().map(|(index, param)| {
                let index = syn::Index::from(index);
                quote_spanned! { param.ty.span()=> ::compose::__private::keep(&#kept.#index) }
            });
            quote! {
                |#kept: &dyn ::core::any::Any| {
                    let #kept = ::compose::__private::kept::<(#(#types,)*)>(#kept);
                    #name #turbofish (#(#arguments),*)
                }
            }
        };
        let skip = if params.is_empty() {
            quote! { #composer.skipping() }
        } else {
            let comparisons = params.iter().enumerate().map(|(index, param)| {
                let index = syn::Index::from(index);
                let name = &param.name;
                quote_spanned! { param.ty.span()=>
                    !::compose::__private::ComposeArg::same(&#kept.#index, &#name)
                }
            });
            let clones = params.iter().map(|param| {
                let name = &param.name;
                quote_spanned! { param.ty.span()=> ::compose::__private::keep(&#name) }
            });
            quote! {
                !#composer.update_kept(
                    |#kept: &(#(#types,)*)| #(#comparisons)||*,
                    || (#(#clones,)*),
                ) && #composer.skipping()
            }
        };
        let restart_name = Ident::new("__compose_restart", Span::mixed_site());
        quote! {
            let #composer = ::compose::__private::current();
            // Also the composable's identity: its type is unique to this composable (and to each
            // instantiation of a generic one), and the same in every copy of an `#[inline]` one.
            let #restart_name = #restart;
            let #group = ::compose::__private::RestartGroup::start(
                #composer,
                #key,
                #restart_name,
                ::compose::__private::identity_of(&#restart_name),
            );
            if #skip {
                #composer.skip_to_group_end();
                return;
            }
            // The body in a block of its own: an item it declares (`use other::Name;`) can't
            // shadow the composable for the restart function above.
            {
                #(#rebinds)*
                #(#stmts)*
            }
        }
    } else {
        quote! {
            let #group = ::compose::__private::ReplaceGroup::start(#key);
            {
                #(#rebinds)*
                #(#stmts)*
            }
        }
    };

    if restartable {
        // The restart closure's `TypeId` identifies the composable: its type parameters must be
        // `'static`, as its kept arguments already are.
        // Inline, next to the parameter's other bounds, so clippy sees one bound location.
        for parameter in function.sig.generics.type_params_mut() {
            parameter.bounds.push(syn::parse_quote!('static));
        }
    }
    let (inner, outer): (Vec<_>, Vec<_>) =
        function.attrs.iter().partition(|attr| matches!(attr.style, syn::AttrStyle::Inner(_)));
    let vis = &function.vis;
    let sig = &function.sig;
    Ok(quote! {
        #(#outer)*
        #[allow(non_snake_case)]
        #vis #sig {
            #(#inner)*
            #body
        }
    })
}
