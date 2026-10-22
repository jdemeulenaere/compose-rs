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
use quote::{ToTokens, TokenStreamExt, quote, quote_spanned};
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
    /// The same, for an optional callback parameter (`Option<impl Fn(…)>`).
    optional: Option<usize>,
    /// An `impl Into<T>` parameter: what it's given is converted.
    into: bool,
    default: Option<TokenStream>,
}

impl Parse for Parameter {
    fn parse(input: ParseStream) -> syn::Result<Parameter> {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let kind: Ident = input.parse()?;
        let arity = if kind == "callback" || kind == "optional" {
            let arity: syn::LitInt = input.parse()?;
            Some(arity.base10_parse()?)
        } else {
            None
        };
        let (callback, optional) = if kind == "optional" { (None, arity) } else { (arity, None) };
        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            let content;
            braced!(content in input);
            Some(content.parse()?)
        } else {
            None
        };
        Ok(Parameter { name, callback, optional, into: kind == "into", default })
    }
}

/// A closure or block the caller's `#[composable]` marked with its captures: its first
/// statement is `::compose::__private::marked!(capture, …);`, which expands to nothing. Bound to
/// a callback parameter, it is memoized on them; bound to another parameter, it is passed as
/// written. A marked block is still a block and a marked closure still a closure, so a user's
/// own PascalCase macro receiving one (as `$body:block`, `|$x:ident| $e:expr`) still works.
pub(crate) struct Marked {
    captures: Vec<Ident>,
    unknown: bool,
    /// Inside another closure: built from clones of its captures (`memo::probe`).
    nested: bool,
    body: MarkedBody,
}

pub(crate) enum MarkedBody {
    Block(syn::Block),
    Closure(syn::ExprClosure),
}

/// A call macro's tokens without the `@path (…)` its caller may have put first, and that
/// prefix.
pub(crate) fn split_path_prefix(tokens: TokenStream) -> (Option<TokenStream>, TokenStream) {
    let mut iter = tokens.clone().into_iter();
    if let (
        Some(proc_macro2::TokenTree::Punct(at)),
        Some(proc_macro2::TokenTree::Ident(word)),
        Some(proc_macro2::TokenTree::Group(group)),
    ) = (iter.next(), iter.next(), iter.next())
        && at.as_char() == '@'
        && word == "path"
    {
        let prefix = quote! { @path #group };
        return (Some(prefix), iter.collect());
    }
    (None, tokens)
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

/// The captures of a marker statement, if `statement` is one.
fn marker_of(statement: &syn::Stmt) -> Option<syn::Result<(Vec<Ident>, bool, bool)>> {
    let syn::Stmt::Macro(statement) = statement else { return None };
    if !is_marker(&statement.mac.path) {
        return None;
    }
    let parser = Punctuated::<Ident, Token![,]>::parse_terminated;
    Some(syn::parse::Parser::parse2(parser, statement.mac.tokens.clone()).map(|captures| {
        let mut captures: Vec<Ident> = captures.into_iter().collect();
        let mut flag = |name: &str| match captures.iter().position(|capture| capture == name) {
            Some(position) => {
                captures.remove(position);
                true
            }
            None => false,
        };
        let unknown = flag("__compose_unknown");
        let nested = flag("__compose_nested");
        (captures, unknown, nested)
    }))
}

impl Marked {
    /// `expression` as a marked closure or block, if it is one.
    pub(crate) fn from_expr(expression: &Expr) -> Option<syn::Result<Marked>> {
        match expression {
            Expr::Block(block) if block.label.is_none() && block.attrs.is_empty() => {
                let first = block.block.stmts.first()?;
                Some(marker_of(first)?.map(|(captures, unknown, nested)| {
                    let mut body = block.block.clone();
                    body.stmts.remove(0);
                    Marked { captures, unknown, nested, body: MarkedBody::Block(body) }
                }))
            }
            Expr::Closure(closure) => {
                let Expr::Block(body) = &*closure.body else { return None };
                let first = body.block.stmts.first()?;
                Some(marker_of(first)?.map(|(captures, unknown, nested)| {
                    let mut closure = closure.clone();
                    let mut rest = body.block.clone();
                    rest.stmts.remove(0);
                    // The marker wrapped the original body in a block: unwrap it.
                    *closure.body = match rest.stmts.as_slice() {
                        [syn::Stmt::Expr(original, None)] => original.clone(),
                        _ => Expr::Block(syn::ExprBlock {
                            attrs: Vec::new(),
                            label: None,
                            block: rest,
                        }),
                    };
                    Marked { captures, unknown, nested, body: MarkedBody::Closure(closure) }
                }))
            }
            _ => None,
        }
    }
}

impl Marked {
    pub(crate) fn new(
        captures: Vec<Ident>,
        unknown: bool,
        nested: bool,
        body: MarkedBody,
    ) -> Marked {
        Marked { captures, unknown, nested, body }
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
        let mut names: Vec<Ident> = captures.clone();
        if self.unknown {
            names.push(Ident::new("__compose_unknown", proc_macro2::Span::call_site()));
        }
        if self.nested {
            names.push(Ident::new("__compose_nested", proc_macro2::Span::call_site()));
        }
        let marker = quote! { ::compose::__private::marked!(#(#names),*); };
        match &self.body {
            MarkedBody::Block(block) => {
                // The block's own braces, so an error about the argument points at it.
                block.brace_token.surround(tokens, |tokens| {
                    marker.to_tokens(tokens);
                    tokens.append_all(&block.stmts);
                });
            }
            MarkedBody::Closure(closure) => {
                let mut closure = closure.clone();
                let marker: syn::Stmt = syn::parse_quote! { #marker };
                let body =
                    std::mem::replace(&mut *closure.body, Expr::Verbatim(TokenStream::new()));
                *closure.body = block_of(marker, body);
                closure.to_tokens(tokens);
            }
        }
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
        match Marked::from_expr(&expression) {
            Some(marked) => Ok(Value::Marked(marked?)),
            None => Ok(Value::Expr(expression)),
        }
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

    /// A composable's call macro (`TopAppBar!(…)`), given where a lambda goes: an error
    /// suggesting the braces of a content lambda.
    fn check_not_a_call(&self, parameter: &Ident) -> syn::Result<()> {
        let Value::Expr(Expr::Macro(call)) = self else { return Ok(()) };
        let Some(name) = call.mac.path.segments.last().map(|segment| segment.ident.to_string())
        else {
            return Ok(());
        };
        if !crate::regions::is_pascal_case(&name) {
            return Ok(());
        }
        Err(Error::new(
            call.mac.path.span(),
            format!(
                "`{parameter}` takes a composable lambda, and `{name}!(…)` is a call: wrap it in \
                 braces, `{parameter} = {{ {name}!(…) }}`"
            ),
        ))
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
    pub(crate) fn value(&self) -> &Value {
        match self {
            Argument::Named(_, value) | Argument::Positional(value) => value,
        }
    }

    pub(crate) fn value_mut(&mut self) -> &mut Value {
        match self {
            Argument::Named(_, value) | Argument::Positional(value) => value,
        }
    }
}

impl Marked {
    pub(crate) fn body(&self) -> &MarkedBody {
        &self.body
    }

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
    /// The path the call macro was invoked by, to call the function by (`ui::Label`).
    path: Option<TokenStream>,
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
        let path = if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            input.parse::<Ident>()?;
            let content;
            parenthesized!(content in input);
            Some(content.parse::<TokenStream>()?)
        } else {
            None
        };
        input.parse::<Token![;]>()?;
        let span = input.span();
        let arguments: Punctuated<Argument, Token![,]> =
            input.parse_terminated(Argument::parse, Token![,])?;
        Ok(Call {
            function,
            path,
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
            let (closure, arity) = match marked.body {
                MarkedBody::Block(block) => (content_closure(arity, &block), arity),
                MarkedBody::Closure(mut closure) => {
                    closure.capture.get_or_insert_with(Default::default);
                    let arity = closure.inputs.len();
                    (closure.into_token_stream(), arity)
                }
            };
            memo::probe(&marked.captures, marked.unknown, closure, Some(arity), marked.nested)
        }
        Value::Expr(Expr::Block(block))
            if block.label.is_none() && !memo::is_probe_block(&block.block) =>
        {
            let closure = content_closure(arity, &block.block);
            let wrap = memo::wrapper(arity);
            quote! { ::compose::__private::#wrap(#closure) }
        }
        Value::Expr(Expr::Closure(mut closure)) => {
            closure.capture.get_or_insert_with(Default::default);
            let wrap = memo::wrapper(closure.inputs.len());
            quote! { ::compose::__private::#wrap(#closure) }
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

/// The value passed for a value parameter: as written, or a clone of a local another argument of
/// the call names: a lambda capturing it (`ModalNavigationDrawer!(drawer_state = drawer, { …
/// drawer … })`), which borrows it to compare its captures, or an expression borrowing it
/// (`Chip!(label, selected = label == current)`). The value can't move it, whatever the
/// parameters' order.
fn shared_value(value: Value, mentions: &std::collections::HashMap<String, usize>) -> TokenStream {
    if let Value::Expr(Expr::Path(path)) = &value
        && path.qself.is_none()
        && let Some(name) = path.path.get_ident()
        && mentions.get(&name.to_string()).is_some_and(|count| *count > 1)
    {
        return quote! { ::core::clone::Clone::clone(&#name) };
    }
    plain_value(value)
}

/// `Some`, as a function called.
pub(crate) fn is_some(function: &Expr) -> bool {
    matches!(function, Expr::Path(path) if path.qself.is_none() && path.path.segments.last().is_some_and(|segment| segment.ident == "Some"))
}

fn is_string_literal(argument: &Argument) -> bool {
    matches!(argument, Argument::Positional(Value::Expr(Expr::Lit(literal))) if matches!(literal.lit, syn::Lit::Str(_)))
}

/// The trailing lambda, taken out of `arguments`: the argument for the last parameter, when that
/// is a callback not given by name, as Kotlin's trailing lambda.
/// - A block or closure after named arguments is it, when it is the only positional argument
///   after them (`Button!(on_click = …, { … }, enabled = false)`).
/// - Otherwise a final block or closure is it, once every required parameter before the last
///   is given: `BasicTextField!(value, move |text| …)` gives `on_value_change`, not the trailing
///   `decoration_box`. Declined because one is missing, it names the first missing one, which
///   the caller reports if the last parameter ends up missing (`Button!({ … })` lacks
///   `on_click`, not `content`).
fn trailing_lambda(
    parameters: &[Parameter],
    arguments: &mut Vec<Argument>,
    formatting: bool,
) -> Trailing {
    let Some(last) = parameters.last() else { return Trailing::None };
    if last.callback.is_none() && last.optional.is_none()
        || arguments
            .iter()
            .any(|argument| matches!(argument, Argument::Named(name, _) if *name == last.name))
    {
        return Trailing::None;
    }
    let first_named = arguments.iter().position(|argument| matches!(argument, Argument::Named(..)));
    let after_named: Vec<usize> = (first_named.map_or(arguments.len(), |first| first + 1)
        ..arguments.len())
        .filter(|index| matches!(arguments[*index], Argument::Positional(_)))
        .collect();
    let index = match after_named.as_slice() {
        [index] if arguments[*index].value().is_lambda() => *index,
        [] if matches!(arguments.last(), Some(Argument::Positional(value)) if value.is_lambda()) => {
            // The positional arguments before it fill the first parameters (a format string
            // and its arguments, the first one).
            let positional = arguments.len() - 1;
            let positional = if formatting { 1 } else { positional };
            let given = |index: usize, parameter: &Parameter| {
                index < positional
                    || arguments.iter().any(
                        |argument| matches!(argument, Argument::Named(name, _) if *name == parameter.name),
                    )
            };
            let missing =
                parameters[..parameters.len() - 1].iter().enumerate().find(|(index, parameter)| {
                    parameter.default.is_none() && !given(*index, parameter)
                });
            if let Some((_, parameter)) = missing {
                let span =
                    arguments.last().map_or_else(Span::call_site, |last| last.value().span());
                return Trailing::Declined(parameter.name.clone(), span);
            }
            arguments.len() - 1
        }
        _ => return Trailing::None,
    };
    Trailing::Lambda(arguments.remove(index).into_value())
}

/// What `trailing_lambda` found.
enum Trailing {
    None,
    Lambda(Value),
    /// A final lambda not taken as the trailing one, as this required parameter is missing.
    Declined(Ident, Span),
}

fn build(call: Call) -> syn::Result<TokenStream> {
    let Call { function, path, parameters, format, mut arguments, span } = call;
    let mut slots: Vec<Option<TokenStream>> = parameters.iter().map(|_| None).collect();
    let set = |slots: &mut Vec<Option<TokenStream>>, index: usize, value: TokenStream| {
        slots[index] = Some(value);
    };

    // The trailing lambda is bound by name: given twice, it is an error as any argument is.
    let formatting = format && arguments.first().is_some_and(is_string_literal);
    let mut declined = None;
    match trailing_lambda(&parameters, &mut arguments, formatting) {
        Trailing::Lambda(value) => {
            if let Some(last) = parameters.last() {
                let name = Ident::new(&last.name.to_string(), value.span());
                arguments.push(Argument::Named(name, value));
            }
        }
        Trailing::Declined(missing, span) => declined = Some((missing, span)),
        Trailing::None => {}
    }

    // How many arguments name each identifier: a local given by itself to a value parameter and
    // named by another argument (captured by a lambda, borrowed by an expression) is cloned for
    // it (`shared_value`).
    let mut mentions: std::collections::HashMap<String, usize> = Default::default();
    for argument in &arguments {
        let mut names = Vec::new();
        memo::mentioned_names(argument.value().to_token_stream(), &mut names);
        names.sort();
        names.dedup();
        for name in names {
            *mentions.entry(name).or_default() += 1;
        }
    }

    // A leading string literal formats into a `#[format]` first parameter, with the positional
    // arguments after it.
    // A named argument the string names (`"{count} items", count = items.len()`) is a format
    // argument, as in `format!`, even if a parameter has its name.
    let mut arguments = arguments.into_iter().peekable();
    let mut next_positional = 0;
    let mut format_arguments = Vec::new();
    let mut format_names = Vec::new();
    if formatting {
        if let Some(Argument::Positional(Value::Expr(Expr::Lit(literal)))) = arguments.peek()
            && let syn::Lit::Str(string) = &literal.lit
        {
            format_names = memo::format_names(&string.value());
        }
        while let Some(Argument::Positional(_)) = arguments.peek() {
            if let Some(argument) = arguments.next() {
                format_arguments.push(plain_value(argument.into_value()));
            }
        }
        next_positional = 1;
    }

    let mut named_seen = false;
    for argument in arguments {
        let argument = match argument {
            Argument::Named(name, value)
                if formatting && format_names.iter().any(|named| name == named) =>
            {
                named_seen = true;
                let value = plain_value(value);
                format_arguments.push(quote! { #name = #value });
                continue;
            }
            argument => argument,
        };
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
                        "positional arguments come before named ones; only the trailing lambda \
                         (`{ … }`) may follow them",
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
        let parameter = &parameters[index];
        let value = match parameter.callback {
            Some(arity) => {
                let value = argument.into_value();
                value.check_not_a_call(&parameter.name)?;
                callback_value(arity, value)
            }
            // A lambda for an optional callback is `Some` callback; anything else (`None`, an
            // `Option` value, a callback or a function by name) is converted.
            None if parameter.optional.is_some() => {
                let arity = parameter.optional.unwrap_or(0);
                let value = argument.into_value();
                value.check_not_a_call(&parameter.name)?;
                match value {
                    value @ (Value::Marked(_)
                    | Value::Expr(Expr::Closure(_))
                    | Value::Expr(Expr::Block(_))) => {
                        let callback = callback_value(arity, value);
                        quote! { ::core::option::Option::Some(#callback) }
                    }
                    // `Some(lambda)`: the same, memoized when marked.
                    Value::Expr(Expr::Call(call))
                        if is_some(&call.func)
                            && call.args.len() == 1
                            && matches!(call.args[0], Expr::Closure(_)) =>
                    {
                        let lambda = call.args[0].clone();
                        let value = match Marked::from_expr(&lambda) {
                            Some(marked) => Value::Marked(marked?),
                            None => Value::Expr(lambda),
                        };
                        let callback = callback_value(arity, value);
                        quote! { ::core::option::Option::Some(#callback) }
                    }
                    // `None` as written (its type comes from the parameter); another `Option`
                    // converted, its closure unmemoized.
                    Value::Expr(Expr::Path(path)) if path.path.is_ident("None") => {
                        path.into_token_stream()
                    }
                    // An optional callback given by name (a function, a callback, a closure, an
                    // `Option`) is memoized on itself, as a callback given by name is: the same
                    // value passes the same callback, and a content lambda passing it on stays
                    // `Fn`.
                    Value::Expr(Expr::Path(path))
                        if path.qself.is_none() && path.path.get_ident().is_some() =>
                    {
                        memo::probe_optional_value(path.into_token_stream())
                    }
                    value => {
                        let value = plain_value(value);
                        quote! { ::compose::__private::optional_callback(#value) }
                    }
                }
            }
            // A `#[format]` parameter converts whatever it is given (`Text!(title)`).
            None if format && index == 0 => {
                let value = shared_value(argument.into_value(), &mentions);
                quote! { ::core::convert::Into::into(#value) }
            }
            // An `impl Into<T>` parameter converts what it's given.
            None if parameter.into => {
                let value = shared_value(argument.into_value(), &mentions);
                quote! { ::core::convert::Into::into(#value) }
            }
            None => shared_value(argument.into_value(), &mentions),
        };
        set(&mut slots, index, value);
    }

    if formatting {
        if slots[0].is_some() {
            return Err(Error::new(span, format!("`{function}`'s text is given twice")));
        }
        let value = quote! {
            ::core::convert::Into::into(::std::format!(#(#format_arguments),*))
        };
        set(&mut slots, 0, value);
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
                if let Some((missing, span)) = &declined {
                    return Err(Error::new(
                        *span,
                        format!("missing argument `{missing}` for `{function}`"),
                    ));
                }
                let message = format!("missing argument `{}` for `{function}`", parameter.name);
                quote_spanned! { span=> ::core::compile_error!(#message) }
            }
        };
        values.push(value);
    }
    match path {
        Some(path) => Ok(quote! { #path(#(#values),*) }),
        None => Ok(quote! { #function(#(#values),*) }),
    }
}

/// `{ first; tail }`, built rather than parsed: syn prints the parentheses a tail needs in
/// statement position (`(if a { 1 } else { 2 }) + base`), which re-parsing the tokens would lose.
pub(crate) fn block_of(first: syn::Stmt, tail: Expr) -> Expr {
    Expr::Block(syn::ExprBlock {
        attrs: Vec::new(),
        label: None,
        block: syn::Block {
            brace_token: Default::default(),
            stmts: vec![first, syn::Stmt::Expr(tail, None)],
        },
    })
}
