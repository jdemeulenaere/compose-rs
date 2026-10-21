//! `call!`: what every composable's call macro expands to (docs/03-composable-macro.md, "Named,
//! default and trailing arguments: the call macro").
//!
//! `Button!(on_click = …, enabled = false, { … })` expands to
//! `::compose::__private::call!{ Button [spec] ; on_click = …, enabled = false, { … } }`, where the
//! spec lists the parameters in order with their kinds and defaults. `call!` binds positional
//! arguments, then named ones; turns a final block or closure into the last parameter when it is
//! a callback; formats a leading string literal into a `#[format]` first parameter; fills
//! defaults; and emits the plain call, `Button(…)`.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Error, Expr, Ident, Token, braced, bracketed, parenthesized};

use crate::memo;

/// One parameter of the spec: `name: callback 1 = { default }` or `name: value`.
struct Parameter {
    name: Ident,
    /// The callback's number of arguments, for a callback parameter.
    callback: Option<usize>,
    default: Option<TokenStream>,
}

impl Parse for Parameter {
    fn parse(input: ParseStream) -> syn::Result<Parameter> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let kind: Ident = input.parse()?;
        let callback = if kind == "callback" {
            let arity: syn::LitInt = input.parse()?;
            Some(arity.base10_parse()?)
        } else {
            None
        };
        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            let content;
            braced!(content in input);
            Some(content.parse()?)
        } else {
            None
        };
        Ok(Parameter { name, callback, default })
    }
}

/// A closure or block the caller's `#[composable]` marked with its captures:
/// `::compose::__private::marked!((capture, …) |x| …)`. Bound to a callback parameter, it is
/// memoized on them; bound to another parameter, it is passed as written. The marker is a macro
/// that expands to its body, so a user's own PascalCase macro receiving it still works.
pub(crate) struct Marked {
    captures: Vec<Ident>,
    unknown: bool,
    body: MarkedBody,
}

pub(crate) enum MarkedBody {
    Block(syn::Block),
    Closure(syn::ExprClosure),
}

impl Parse for Marked {
    fn parse(input: ParseStream) -> syn::Result<Marked> {
        let content;
        parenthesized!(content in input);
        let captures: Punctuated<Ident, Token![,]> =
            Punctuated::parse_terminated_with(&content, |stream: ParseStream| {
                stream.parse::<Ident>()
            })?;
        let mut unknown = false;
        let mut captures: Vec<Ident> = captures.into_iter().collect();
        if let Some(position) = captures.iter().position(|capture| capture == "__compose_unknown") {
            captures.remove(position);
            unknown = true;
        }
        let body = if input.peek(syn::token::Brace) {
            MarkedBody::Block(input.parse()?)
        } else {
            MarkedBody::Closure(input.parse()?)
        };
        Ok(Marked { captures, unknown, body })
    }
}

/// Whether `path` is `::compose::__private::marked`.
fn is_marker(path: &syn::Path) -> bool {
    path.leading_colon.is_some()
        && path.segments.len() == 3
        && path
            .segments
            .iter()
            .zip(["compose", "__private", "marked"])
            .all(|(segment, name)| segment.ident == name)
}

impl Marked {
    pub(crate) fn new(captures: Vec<Ident>, unknown: bool, body: MarkedBody) -> Marked {
        Marked { captures, unknown, body }
    }

    fn span(&self) -> Span {
        match &self.body {
            MarkedBody::Block(block) => block.span(),
            MarkedBody::Closure(closure) => closure.span(),
        }
    }
}

impl ToTokens for Marked {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let captures = &self.captures;
        let unknown = self.unknown.then(|| quote!(__compose_unknown));
        let separator = (self.unknown && !captures.is_empty()).then(|| quote!(,));
        let body = match &self.body {
            MarkedBody::Block(block) => block.to_token_stream(),
            MarkedBody::Closure(closure) => closure.to_token_stream(),
        };
        tokens.extend(
            quote! { ::compose::__private::marked!((#(#captures),* #separator #unknown) #body) },
        );
    }
}

pub(crate) enum Value {
    Expr(Expr),
    Marked(Marked),
}

impl ToTokens for Value {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Value::Expr(expression) => expression.to_tokens(tokens),
            Value::Marked(marked) => marked.to_tokens(tokens),
        }
    }
}

impl Parse for Value {
    fn parse(input: ParseStream) -> syn::Result<Value> {
        let expression: Expr = input.parse()?;
        if let Expr::Macro(mac) = &expression
            && is_marker(&mac.mac.path)
        {
            return Ok(Value::Marked(syn::parse2(mac.mac.tokens.clone())?));
        }
        Ok(Value::Expr(expression))
    }
}

impl Value {
    fn span(&self) -> Span {
        match self {
            Value::Expr(expression) => expression.span(),
            Value::Marked(marked) => marked.span(),
        }
    }

    /// A block or a closure: what a trailing lambda is.
    fn is_lambda(&self) -> bool {
        matches!(
            self,
            Value::Marked(_) | Value::Expr(Expr::Block(_)) | Value::Expr(Expr::Closure(_))
        )
    }
}

pub(crate) enum Argument {
    Named(Ident, Value),
    Positional(Value),
}

impl ToTokens for Argument {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Argument::Named(name, value) => tokens.extend(quote! { #name = #value }),
            Argument::Positional(value) => value.to_tokens(tokens),
        }
    }
}

impl Argument {
    pub(crate) fn value_mut(&mut self) -> &mut Value {
        match self {
            Argument::Named(_, value) | Argument::Positional(value) => value,
        }
    }
}

impl Marked {
    pub(crate) fn body_mut(&mut self) -> &mut MarkedBody {
        &mut self.body
    }
}

impl Parse for Argument {
    fn parse(input: ParseStream) -> syn::Result<Argument> {
        if input.peek(Ident) && input.peek2(Token![=]) && !input.peek2(Token![==]) {
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            return Ok(Argument::Named(name, input.parse()?));
        }
        Ok(Argument::Positional(input.parse()?))
    }
}

impl Argument {
    fn into_value(self) -> Value {
        match self {
            Argument::Named(_, value) | Argument::Positional(value) => value,
        }
    }
}

struct Call {
    function: Ident,
    parameters: Vec<Parameter>,
    format: bool,
    arguments: Vec<Argument>,
    span: Span,
}

impl Parse for Call {
    fn parse(input: ParseStream) -> syn::Result<Call> {
        let function: Ident = input.parse()?;
        let content;
        bracketed!(content in input);
        let parameters: Punctuated<Parameter, Token![,]> =
            content.parse_terminated(Parameter::parse, Token![,])?;
        let flag: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        let span = input.span();
        let arguments: Punctuated<Argument, Token![,]> =
            input.parse_terminated(Argument::parse, Token![,])?;
        Ok(Call {
            function,
            parameters: parameters.into_iter().collect(),
            format: flag == "format",
            arguments: arguments.into_iter().collect(),
            span,
        })
    }
}

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    match syn::parse2::<Call>(input).and_then(build) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// A closure taking `arity` ignored arguments around a content block.
fn content_closure(arity: usize, block: &syn::Block) -> TokenStream {
    let underscores = (0..arity).map(|_| quote!(_));
    quote! { move |#(#underscores),*| #block }
}

/// The value passed for a callback parameter: a marked lambda memoized on its captures, an
/// unmarked one (outside a composable) wrapped, `move` added either way; anything else converted.
fn callback_value(arity: usize, value: Value) -> TokenStream {
    match value {
        Value::Marked(marked) => {
            let closure = match marked.body {
                MarkedBody::Block(block) => content_closure(arity, &block),
                MarkedBody::Closure(mut closure) => {
                    closure.capture.get_or_insert_with(Default::default);
                    closure.into_token_stream()
                }
            };
            memo::probe(&marked.captures, marked.unknown, closure)
        }
        Value::Expr(Expr::Block(block)) if block.label.is_none() => {
            let closure = content_closure(arity, &block.block);
            quote! { ::compose::__private::callback(#closure) }
        }
        Value::Expr(Expr::Closure(mut closure)) => {
            closure.capture.get_or_insert_with(Default::default);
            quote! { ::compose::__private::callback(#closure) }
        }
        // A callback given by name (`on_click = on_remove`) is cloned, not moved, since the call
        // may sit in a content lambda that runs again; and memoized on itself.
        Value::Expr(expression) => memo::probe_value(expression.into_token_stream()),
    }
}

/// The value passed for another parameter: as written, markers dropped.
fn plain_value(value: Value) -> TokenStream {
    match value {
        Value::Marked(marked) => match marked.body {
            MarkedBody::Block(block) => block.into_token_stream(),
            MarkedBody::Closure(closure) => closure.into_token_stream(),
        },
        Value::Expr(expression) => expression.into_token_stream(),
    }
}

fn is_string_literal(argument: &Argument) -> bool {
    matches!(argument, Argument::Positional(Value::Expr(Expr::Lit(literal))) if matches!(literal.lit, syn::Lit::Str(_)))
}

fn build(call: Call) -> syn::Result<TokenStream> {
    let Call { function, parameters, format, mut arguments, span } = call;
    let mut slots: Vec<Option<TokenStream>> = parameters.iter().map(|_| None).collect();
    let set = |slots: &mut Vec<Option<TokenStream>>, index: usize, value: TokenStream| {
        slots[index] = Some(value);
    };

    // A final block or closure is the last parameter, when that is a callback (a trailing lambda).
    let mut trailing = None;
    let last_is_callback = parameters.last().is_some_and(|parameter| parameter.callback.is_some());
    if last_is_callback {
        let is_trailing =
            matches!(arguments.last(), Some(Argument::Positional(value)) if value.is_lambda());
        let named_last = arguments.iter().any(|argument| {
            matches!(argument, Argument::Named(name, _) if parameters.last().is_some_and(|last| *name == last.name))
        });
        if is_trailing && !named_last {
            trailing = arguments.pop();
        }
    }

    // A leading string literal formats into a `#[format]` first parameter, with the positional
    // arguments after it.
    let mut arguments = arguments.into_iter().peekable();
    let mut next_positional = 0;
    if format && arguments.peek().is_some_and(is_string_literal) {
        let mut format_arguments = Vec::new();
        while let Some(Argument::Positional(_)) = arguments.peek() {
            if let Some(argument) = arguments.next() {
                format_arguments.push(plain_value(argument.into_value()));
            }
        }
        let value = quote! {
            ::core::convert::Into::into(::std::format!(#(#format_arguments),*))
        };
        set(&mut slots, 0, value);
        next_positional = 1;
    }

    let mut named_seen = false;
    for argument in arguments {
        let index = match &argument {
            Argument::Named(name, _) => {
                named_seen = true;
                let Some(index) = parameters.iter().position(|parameter| parameter.name == *name)
                else {
                    let names: Vec<String> =
                        parameters.iter().map(|parameter| parameter.name.to_string()).collect();
                    return Err(Error::new(
                        name.span(),
                        format!(
                            "`{function}` has no parameter `{name}`; its parameters are: {}",
                            names.join(", ")
                        ),
                    ));
                };
                if slots[index].is_some() {
                    return Err(Error::new(
                        name.span(),
                        format!("argument `{name}` of `{function}` is given twice"),
                    ));
                }
                index
            }
            Argument::Positional(value) => {
                if named_seen {
                    return Err(Error::new(
                        value.span(),
                        "positional arguments come before named ones",
                    ));
                }
                if next_positional >= parameters.len() {
                    return Err(Error::new(
                        value.span(),
                        format!("too many arguments for `{function}`"),
                    ));
                }
                next_positional += 1;
                next_positional - 1
            }
        };
        let value = match parameters[index].callback {
            Some(arity) => callback_value(arity, argument.into_value()),
            // A `#[format]` parameter converts whatever it is given (`Text!(title)`).
            None if format && index == 0 => {
                let value = plain_value(argument.into_value());
                quote! { ::core::convert::Into::into(#value) }
            }
            None => plain_value(argument.into_value()),
        };
        set(&mut slots, index, value);
    }

    if let Some(argument) = trailing {
        let index = parameters.len() - 1;
        if slots[index].is_some() {
            return Err(Error::new(span, format!("`{function}`'s last argument is given twice")));
        }
        let arity = parameters[index].callback.unwrap_or(0);
        set(&mut slots, index, callback_value(arity, argument.into_value()));
    }

    let mut values = Vec::new();
    for (parameter, slot) in parameters.iter().zip(slots) {
        let value = match (slot, &parameter.default) {
            (Some(value), _) => value,
            (None, Some(default)) => match parameter.callback {
                Some(_) => memo::probe_value(default.clone()),
                None => default.clone(),
            },
            (None, None) => {
                let message = format!("missing argument `{}` for `{function}`", parameter.name);
                quote_spanned! { span=> ::core::compile_error!(#message) }
            }
        };
        values.push(value);
    }
    Ok(quote! { #function(#(#values),*) })
}
