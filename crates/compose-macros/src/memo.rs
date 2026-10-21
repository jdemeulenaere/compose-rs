//! Lambda memoization (docs/03-composable-macro.md, "Lambdas: memoized on their captures").
//!
//! A closure passed to a composable call becomes a `Callback` remembered at its position, and
//! replaced only when one of its captures is not `same` as last pass: the callee sees the same
//! callback and can skip. The captures are found syntactically: the identifiers in the closure
//! that name locals of the enclosing function visible at that point, tracked through `let`,
//! parameters, `for`, `match` arms, `if let` and closure parameters. A real capture must never be
//! missed, so identifiers in macro arguments and format strings (`"{count}"`, `"{:width$}"`)
//! count; a closure calling an unknown macro, or naming an identifier given to an unknown
//! statement macro (which may bind it), counts as always changed.
//!
//! The macro sees syntax, not types, so it only rewrites closures it knows become callbacks:
//! - in a composable's call macro, every closure and block argument is marked
//!   `::compose::__private::marked!((captures) …)`, and the call macro memoizes those bound to
//!   callback parameters (it knows its parameters and their arities), a block becoming a
//!   content lambda; a callback given as a value (a name, a default) is memoized on itself;
//! - elsewhere, a closure wrapped as `callback(…)` or `Callback::new(…)` is memoized.
//!
//! Closures passed to other functions (`map`, `update`, `key`, a generic `F: Fn()` parameter)
//! keep their types.

use std::collections::HashSet;

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::visit_mut::{self, VisitMut};
use syn::{Expr, Pat, Token, parse_quote};

use crate::call::{Marked, MarkedBody};

/// Macros that can't reach a local that isn't in their tokens: standard macros, and composables'
/// call macros (declarative, hence hygienic).
const KNOWN_MACROS: &[&str] = &[
    "format",
    "format_args",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "vec",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "panic",
    "todo",
    "unimplemented",
    "unreachable",
    "matches",
    "dbg",
    "concat",
    "stringify",
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    "log",
];

fn is_call_macro(name: &str) -> bool {
    crate::regions::is_pascal_case(name)
}

/// Whether `receiver` is a modifier chain: method calls on the path `Modifier`.
fn is_modifier_chain(receiver: &Expr) -> bool {
    match receiver {
        Expr::MethodCall(call) => is_modifier_chain(&call.receiver),
        Expr::Path(path) => {
            path.qself.is_none()
                && path.path.segments.last().is_some_and(|segment| segment.ident == "Modifier")
        }
        Expr::Paren(paren) => is_modifier_chain(&paren.expr),
        _ => false,
    }
}

/// `remember(…)` and its variants.
fn is_remember(function: &Expr) -> bool {
    matches!(function, Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident.to_string().starts_with("remember")))
}

/// `callback(…)` or `Callback::new(…)`: a closure explicitly made a callback.
fn is_callback_constructor(path: &syn::Path) -> bool {
    let segments: Vec<String> =
        path.segments.iter().map(|segment| segment.ident.to_string()).collect();
    match segments.as_slice() {
        [.., last] if last == "callback" => true,
        [.., ty, last] => ty == "Callback" && last == "new",
        _ => false,
    }
}

/// Keys as a tuple, nested in tuples of at most 12, which is as far as `LambdaKeys` goes.
fn key_tuple(keys: Vec<TokenStream>) -> TokenStream {
    if keys.len() <= 12 {
        return quote! { (#(#keys,)*) };
    }
    let chunks: Vec<TokenStream> = keys.chunks(12).map(|chunk| key_tuple(chunk.to_vec())).collect();
    key_tuple(chunks)
}

/// The runtime function resolving a probe for a lambda of `arity` arguments: one spelling the
/// `Fn` bound out, so untyped closure parameters are inferred, up to four.
fn resolver(arity: Option<usize>) -> Option<Ident> {
    arity.filter(|arity| *arity <= 4).map(|arity| format_ident!("resolve_{}", arity))
}

/// The runtime function wrapping a lambda of `arity` arguments in a `Callback`, as `resolver`.
pub(crate) fn wrapper(arity: usize) -> Ident {
    if arity <= 4 { format_ident!("callback_{}", arity) } else { format_ident!("callback") }
}

/// A lookup of the lambda remembered at this position, keyed with `keys`: the kept callback if
/// they are all `same` as last pass's, else `make` stored.
fn probe_with(keys: Vec<TokenStream>, make: TokenStream, arity: Option<usize>) -> TokenStream {
    let keys = key_tuple(keys);
    let resolve = match resolver(arity) {
        Some(resolver) => {
            quote! { ::compose::__private::#resolver(::compose::__private::lambda_probe(#keys), #make) }
        }
        None => quote! { ::compose::__private::lambda_probe(#keys).resolve(#make) },
    };
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, UnkeyedProbe as _};
        #resolve
    }}
}

/// The memoized form of `closure`: a probe of the lambda remembered at this position.
///
/// A lambda nested in another closure (a content block inside a content block) can't move its
/// captures out of it: it is built only on a miss, from clones of its captures.
pub(crate) fn probe(
    captures: &[Ident],
    unknown: bool,
    closure: TokenStream,
    arity: Option<usize>,
    nested: bool,
) -> TokenStream {
    let mut keys: Vec<TokenStream> = captures
        .iter()
        .map(|capture| quote! { (&&::compose::__private::ArgProbe(&#capture)).arg_key() })
        .collect();
    if unknown {
        keys.push(quote! { ::compose::__private::Unkeyed::CHANGED });
    }
    if !nested {
        return probe_with(keys, closure, arity);
    }
    let keys = key_tuple(keys);
    let resolver = match arity.filter(|arity| *arity <= 4) {
        Some(arity) => format_ident!("resolve_lazy_{}", arity),
        None => format_ident!("resolve_lazy"),
    };
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, UnkeyedProbe as _};
        ::compose::__private::#resolver(::compose::__private::lambda_probe(#keys), || {
            #(let #captures = ::core::clone::Clone::clone(&#captures);)*
            #closure
        })
    }}
}

/// A callback given as a value (`on_click = on_remove`, a default): memoized on the value
/// itself, so the same callback, function or capture-less closure passes the same `Callback`.
pub(crate) fn probe_value(value: TokenStream) -> TokenStream {
    let name = Ident::new("__compose_value", Span::mixed_site());
    let key = quote! { (&&::compose::__private::ArgProbe(#name)).arg_key() };
    let keys = key_tuple(vec![key]);
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, UnkeyedProbe as _};
        let #name = &(#value);
        ::compose::__private::lambda_probe(#keys).resolve_cloned(#name)
    }}
}

/// The names a parameter's pattern binds.
/// The names a `let` binds: refutable when it has an `else`.
fn local_bindings(local: &syn::Local, out: &mut Vec<String>) {
    if local.init.as_ref().is_some_and(|init| init.diverge.is_some()) {
        refutable_bindings(&local.pat, out)
    } else {
        bindings(&local.pat, out)
    }
}

/// The snake-case identifiers directly before a `=` (not `==`) in a macro's top-level tokens.
fn assigned_names(tokens: TokenStream) -> Vec<String> {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut names = Vec::new();
    for window in tokens.windows(3) {
        if let (TokenTree::Ident(ident), TokenTree::Punct(equals)) = (&window[0], &window[1]) {
            let next_is_equals =
                matches!(&window[2], TokenTree::Punct(next) if next.as_char() == '=');
            let name = ident.to_string();
            if equals.as_char() == '='
                && equals.spacing() == proc_macro2::Spacing::Alone
                && !next_is_equals
                && !crate::regions::is_pascal_case(&name)
            {
                names.push(name);
            }
        }
    }
    if let [.., TokenTree::Ident(ident), TokenTree::Punct(equals)] = tokens.as_slice()
        && equals.as_char() == '='
    {
        names.push(ident.to_string());
    }
    names
}

/// The identifiers in a macro's tokens.
fn token_idents(tokens: TokenStream, out: &mut Vec<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => out.push(ident.to_string()),
            TokenTree::Group(group) => token_idents(group.stream(), out),
            _ => {}
        }
    }
}

fn is_known_macro(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| KNOWN_MACROS.contains(&segment.ident.to_string().as_str()))
}

fn is_call_macro_path(path: &syn::Path) -> bool {
    path.segments.last().is_some_and(|segment| is_call_macro(&segment.ident.to_string()))
}

/// A call macro invoked by path (`ui::Label!(…)`): its expansion calls the bare name, so the
/// path is imported around it instead (`{ use ui::Label; Label!(…) }`), and the call reaches
/// the function the path names. Returns the path to import, leaving the bare name.
fn unqualify(mac: &mut syn::Macro) -> Option<syn::Path> {
    if mac.path.segments.len() < 2 {
        return None;
    }
    let path = mac.path.clone();
    let last = path.segments.last().expect("a segment").ident.clone();
    mac.path = syn::Path::from(last);
    Some(path)
}

pub(crate) fn pattern_names(pattern: &Pat, out: &mut Vec<String>) {
    bindings(pattern, out)
}

/// The names an irrefutable pattern binds (`let`, parameters, `for`, closure parameters): every
/// identifier, PascalCase ones included (a unit struct there is harmless to take for a local).
fn bindings(pattern: &Pat, out: &mut Vec<String>) {
    pattern_bindings(pattern, out, false)
}

/// The names a refutable pattern binds (`match` arms, `if let`, `let … else`). A PascalCase
/// identifier there may be a constant or a unit variant, or a binding: it is bound *tainted*
/// (marked with a leading `TAINTED`), so a lambda naming it counts as always changed. `None` is
/// the prelude's variant.
fn refutable_bindings(pattern: &Pat, out: &mut Vec<String>) {
    pattern_bindings(pattern, out, true)
}

fn pattern_bindings(pattern: &Pat, out: &mut Vec<String>, refutable: bool) {
    struct Bindings<'a>(&'a mut Vec<String>, bool);
    impl<'ast> Visit<'ast> for Bindings<'_> {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            let name = pattern.ident.to_string();
            if !self.1 || !crate::regions::is_pascal_case(&name) {
                self.0.push(name);
            } else if name != "None" {
                self.0.push(format!("{TAINTED}{name}"));
            }
            visit::visit_pat_ident(self, pattern);
        }
        fn visit_expr(&mut self, _: &'ast Expr) {
            // Range bounds and constants in patterns bind nothing.
        }
    }
    Bindings(out, refutable).visit_pat(pattern);
}

/// Names bound by `let` conditions (`if let`, `while let`, let chains).
fn condition_bindings(condition: &Expr, out: &mut Vec<String>) {
    match condition {
        Expr::Let(binding) => refutable_bindings(&binding.pat, out),
        Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_)) => {
            condition_bindings(&binary.left, out);
            condition_bindings(&binary.right, out);
        }
        Expr::Paren(paren) => condition_bindings(&paren.expr, out),
        _ => {}
    }
}

/// Marks a bound name as tainted in binding lists.
const TAINTED: char = '\u{1}';

fn binding(name: String) -> (String, bool) {
    match name.strip_prefix(TAINTED) {
        Some(name) => (name.to_string(), true),
        None => (name, false),
    }
}

/// Scopes of local names, innermost last. A tainted name may be a local bound by a macro the
/// walk can't see into.
#[derive(Clone, Default)]
pub(crate) struct Scopes(Vec<Vec<(String, bool)>>);

impl Scopes {
    fn push(&mut self, names: Vec<String>) {
        self.0.push(names.into_iter().map(binding).collect());
    }

    fn pop(&mut self) {
        self.0.pop();
    }

    fn bind(&mut self, names: Vec<String>) {
        self.0.last_mut().expect("a scope").extend(names.into_iter().map(binding));
    }

    fn taint(&mut self, names: Vec<String>) {
        self.0.last_mut().expect("a scope").extend(names.into_iter().map(|name| (name, true)));
    }

    /// Whether `name` is bound, and if so whether its innermost binding is tainted.
    fn lookup(&self, name: &str) -> Option<bool> {
        self.0.iter().rev().find_map(|scope| {
            scope.iter().rev().find(|(bound, _)| bound == name).map(|(_, tainted)| *tainted)
        })
    }

    fn contains(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }
}

/// The captures of a closure or block: free identifiers naming locals of `outer`.
struct Free<'a> {
    outer: &'a Scopes,
    /// `macro_rules!` defined in the function: they can name its locals.
    local_macros: &'a [String],
    inner: Scopes,
    found: Vec<Ident>,
    seen: HashSet<String>,
    unknown: bool,
}

impl Free<'_> {
    fn use_name(&mut self, ident: &Ident) {
        let name = ident.to_string();
        if self.inner.contains(&name) {
            return;
        }
        match self.outer.lookup(&name) {
            Some(true) => self.unknown = true,
            Some(false) if self.seen.insert(name) => self.found.push(ident.clone()),
            _ => {}
        }
    }

    /// Whether a macro can't reach a local that isn't in its tokens.
    fn trusted(&self, name: &str) -> bool {
        // `marked` is the call-macro marker this pass writes.
        (KNOWN_MACROS.contains(&name) || crate::regions::is_pascal_case(name) || name == "marked")
            && !self.local_macros.iter().any(|local| local == name)
    }

    fn scan_tokens(&mut self, tokens: TokenStream) {
        let mut previous: Option<Ident> = None;
        for token in tokens {
            match token {
                TokenTree::Ident(ident) => {
                    self.use_name(&ident);
                    previous = Some(ident);
                    continue;
                }
                TokenTree::Group(group) => self.scan_tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    for name in format_names(&literal.to_string()) {
                        // The literal's span: the name resolves where the string was written.
                        self.use_name(&Ident::new(&name, literal.span()));
                    }
                }
                TokenTree::Punct(punct) => {
                    // A macro called inside a macro's tokens.
                    if punct.as_char() == '!'
                        && let Some(name) = &previous
                        && !self.trusted(&name.to_string())
                    {
                        self.unknown = true;
                    }
                }
            }
            previous = None;
        }
    }

    fn scoped(&mut self, names: Vec<String>, f: impl FnOnce(&mut Self)) {
        self.inner.push(names);
        f(self);
        self.inner.pop();
    }
}

/// The names a format string refers to: inline arguments (`"{count}"`, `"{value:>8}"`) and
/// width and precision arguments (`"{:>width$}"`, `"{:.prec$}"`). Positional indices (`"{0}"`)
/// aren't names.
fn format_names(literal: &str) -> Vec<String> {
    if !literal.starts_with('"') && !literal.starts_with('r') {
        return Vec::new();
    }
    let is_name = |name: &str| {
        name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    };
    let mut names = Vec::new();
    let mut chars = literal.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            continue;
        }
        let placeholder: String = chars.by_ref().take_while(|c| *c != '}').collect();
        let (argument, spec) = placeholder.split_once(':').unwrap_or((&placeholder, ""));
        let argument = argument.trim();
        if is_name(argument) {
            names.push(argument.to_string());
        }
        // `name$` in the spec: a width or precision taken from an argument.
        let mut word = String::new();
        for c in spec.chars() {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
            } else {
                if c == '$' && is_name(&word) {
                    names.push(word.clone());
                }
                word.clear();
            }
        }
    }
    names
}

impl<'ast> Visit<'ast> for Free<'_> {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if path.qself.is_none()
            && let Some(ident) = path.path.get_ident()
        {
            self.use_name(ident);
        }
        visit::visit_expr_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let name = mac.path.segments.last().map(|segment| segment.ident.to_string());
        if !name.as_deref().is_some_and(|name| self.trusted(name)) {
            self.unknown = true;
        }
        // A call macro's arguments are expressions: visited as such, a named argument's name
        // (`modifier = …`) isn't a use, and names bound inside its lambdas are scoped.
        if name.as_deref().is_some_and(is_call_macro) {
            let parser = Punctuated::<crate::call::Argument, Token![,]>::parse_terminated;
            if let Ok(arguments) = syn::parse::Parser::parse2(parser, mac.tokens.clone()) {
                for argument in &arguments {
                    match argument.value() {
                        crate::call::Value::Expr(expression) => self.visit_expr(expression),
                        crate::call::Value::Marked(marked) => match marked.body() {
                            MarkedBody::Block(block) => self.visit_block(block),
                            MarkedBody::Closure(closure) => self.visit_expr_closure(closure),
                        },
                    }
                }
                return;
            }
        }
        self.scan_tokens(mac.tokens.clone());
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.scoped(Vec::new(), |free| {
            for stmt in &block.stmts {
                match stmt {
                    syn::Stmt::Local(local) => {
                        if let Some(init) = &local.init {
                            free.visit_expr(&init.expr);
                            if let Some((_, otherwise)) = &init.diverge {
                                free.visit_expr(otherwise);
                            }
                        }
                        let mut names = Vec::new();
                        local_bindings(local, &mut names);
                        free.inner.bind(names);
                    }
                    syn::Stmt::Item(_) => {}
                    other => free.visit_stmt(other),
                }
            }
        });
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        let mut names = Vec::new();
        for input in &closure.inputs {
            bindings(input, &mut names);
        }
        self.scoped(names, |free| free.visit_expr(&closure.body));
    }

    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        let mut names = Vec::new();
        bindings(&expr.pat, &mut names);
        self.scoped(names, |free| free.visit_block(&expr.body));
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let mut names = Vec::new();
        refutable_bindings(&arm.pat, &mut names);
        self.scoped(names, |free| {
            if let Some((_, guard)) = &arm.guard {
                free.visit_expr(guard);
            }
            free.visit_expr(&arm.body);
        });
    }

    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        self.visit_expr(&expr.cond);
        let mut names = Vec::new();
        condition_bindings(&expr.cond, &mut names);
        self.scoped(names, |free| free.visit_block(&expr.then_branch));
        if let Some((_, otherwise)) = &expr.else_branch {
            self.visit_expr(otherwise);
        }
    }

    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.visit_expr(&expr.cond);
        let mut names = Vec::new();
        condition_bindings(&expr.cond, &mut names);
        self.scoped(names, |free| free.visit_block(&expr.body));
    }

    fn visit_expr_let(&mut self, expr: &'ast syn::ExprLet) {
        self.visit_expr(&expr.expr);
    }

    fn visit_item(&mut self, _: &'ast syn::Item) {}
}

/// The captures of `node` among the locals of `outer`, and whether it calls an unknown macro.
fn captures_of(
    outer: &Scopes,
    local_macros: &[String],
    visit: impl FnOnce(&mut Free<'_>),
) -> (Vec<Ident>, bool) {
    let mut free = Free {
        outer,
        local_macros,
        inner: Scopes::default(),
        found: Vec::new(),
        seen: HashSet::new(),
        unknown: false,
    };
    visit(&mut free);
    (free.found, free.unknown)
}

/// A closure made a callback, memoized on its captures; `move` is added, as a callback is
/// `'static`.
fn memoize_closure(
    nested: bool,
    scopes: &Scopes,
    local_macros: &[String],
    closure: &mut syn::ExprClosure,
) -> Expr {
    let (captures, unknown) =
        captures_of(scopes, local_macros, |free| free.visit_expr_closure(closure));
    closure.capture.get_or_insert_with(Default::default);
    let tokens =
        probe(&captures, unknown, closure.to_token_stream(), Some(closure.inputs.len()), nested);
    syn::parse2(tokens).expect("a probe parses")
}

/// An argument of a call macro, as `call!` parses it.
enum MacroArgument {
    Named(Ident, Token![=], Expr),
    Positional(Expr),
}

impl syn::parse::Parse for MacroArgument {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<MacroArgument> {
        if input.peek(syn::Ident) && input.peek2(Token![=]) && !input.peek2(Token![==]) {
            return Ok(MacroArgument::Named(input.parse()?, input.parse()?, input.parse()?));
        }
        Ok(MacroArgument::Positional(input.parse()?))
    }
}

pub(crate) struct Memoize {
    scopes: Scopes,
    local_macros: Vec<String>,
    /// How many closures (or content blocks) enclose the code being visited.
    depth: usize,
}

impl Memoize {
    pub fn new(parameters: Vec<String>, body: &syn::Block) -> Memoize {
        let mut scopes = Scopes::default();
        scopes.push(parameters);
        // `macro_rules!` items in the body, at any depth: hygiene lets them name the locals in
        // scope where they're defined, so they're never trusted.
        struct LocalMacros(Vec<String>);
        impl<'ast> Visit<'ast> for LocalMacros {
            fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
                if let Some(ident) = &item.ident {
                    self.0.push(ident.to_string());
                }
            }
        }
        let mut local_macros = LocalMacros(Vec::new());
        local_macros.visit_block(body);
        Memoize { scopes, local_macros: local_macros.0, depth: 0 }
    }

    /// A call macro argument: a closure or a block marked with its captures,
    /// anything else visited.
    fn mark(&mut self, expression: &mut Expr, tokens: &mut TokenStream) {
        match expression {
            Expr::Closure(closure) => {
                // Inner calls first, with the closure's parameters in scope.
                self.visit_expr_closure_mut(closure);
                let (captures, unknown) = captures_of(&self.scopes, &self.local_macros, |free| {
                    free.visit_expr_closure(closure)
                });
                Marked::new(
                    captures,
                    unknown,
                    self.depth > 0,
                    MarkedBody::Closure(closure.clone()),
                )
                .to_tokens(tokens);
            }
            Expr::Block(block) if block.label.is_none() && block.attrs.is_empty() => {
                // A block becomes a content lambda: what's inside is nested.
                self.depth += 1;
                self.visit_block_mut(&mut block.block);
                self.depth -= 1;
                let (captures, unknown) = captures_of(&self.scopes, &self.local_macros, |free| {
                    free.visit_block(&block.block)
                });
                Marked::new(
                    captures,
                    unknown,
                    self.depth > 0,
                    MarkedBody::Block(block.block.clone()),
                )
                .to_tokens(tokens);
            }
            other => {
                self.visit_expr_mut(other);
                other.to_tokens(tokens);
            }
        }
    }

    /// A composable's call macro: its closures and its trailing block marked.
    fn rewrite_call_macro(&mut self, mac: &mut syn::Macro) {
        let name = mac.path.segments.last().map(|segment| segment.ident.to_string());
        if !name.as_deref().is_some_and(is_call_macro) || mac.tokens.is_empty() {
            return;
        }
        let parser = Punctuated::<MacroArgument, Token![,]>::parse_terminated;
        let Ok(arguments) = syn::parse::Parser::parse2(parser, mac.tokens.clone()) else {
            return;
        };
        let mut arguments: Vec<MacroArgument> = arguments.into_iter().collect();
        let mut tokens = TokenStream::new();
        for (index, argument) in arguments.iter_mut().enumerate() {
            if index > 0 {
                tokens.extend(quote!(,));
            }
            match argument {
                MacroArgument::Positional(expression) => {
                    self.mark(expression, &mut tokens);
                }
                MacroArgument::Named(name, equals, expression) => {
                    name.to_tokens(&mut tokens);
                    equals.to_tokens(&mut tokens);
                    self.mark(expression, &mut tokens);
                }
            }
        }
        mac.tokens = tokens;
    }

    fn scoped(&mut self, names: Vec<String>, f: impl FnOnce(&mut Self)) {
        self.scopes.push(names);
        f(self);
        self.scopes.pop();
    }
}

impl VisitMut for Memoize {
    fn visit_item_mut(&mut self, _: &mut syn::Item) {}

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        self.scoped(Vec::new(), |memoize| {
            for stmt in &mut block.stmts {
                match stmt {
                    syn::Stmt::Local(local) => {
                        if let Some(init) = &mut local.init {
                            memoize.visit_expr_mut(&mut init.expr);
                            if let Some((_, otherwise)) = &mut init.diverge {
                                memoize.visit_expr_mut(otherwise);
                            }
                        }
                        let mut names = Vec::new();
                        local_bindings(local, &mut names);
                        memoize.scopes.bind(names);
                    }
                    syn::Stmt::Macro(statement) => {
                        let name = statement
                            .mac
                            .path
                            .segments
                            .last()
                            .map(|segment| segment.ident.to_string());
                        let local =
                            name.as_ref().is_some_and(|name| memoize.local_macros.contains(name));
                        if is_call_macro_path(&statement.mac.path) && !local {
                            // A PascalCase macro may be the user's, binding the names it's given
                            // (`Bind!(current = …)`): a new name before `=` is tainted. Names of
                            // locals, and every other identifier, keep their meaning.
                            let assigned = assigned_names(statement.mac.tokens.clone())
                                .into_iter()
                                .filter(|name| !memoize.scopes.contains(name))
                                .collect();
                            memoize.scopes.taint(assigned);
                            memoize.rewrite_call_macro(&mut statement.mac);
                            if let Some(path) = unqualify(&mut statement.mac) {
                                let attrs = std::mem::take(&mut statement.attrs);
                                let mac = &statement.mac;
                                let semi = statement.semi_token;
                                *stmt = syn::Stmt::Expr(
                                    parse_quote! { #(#attrs)* { use #path; #mac } },
                                    semi,
                                );
                            }
                        } else if !is_known_macro(&statement.mac.path) {
                            // It may bind locals the walk can't see: a lambda naming any
                            // identifier it was given is always changed.
                            let mut names = Vec::new();
                            token_idents(statement.mac.tokens.clone(), &mut names);
                            memoize.scopes.taint(names);
                        }
                    }
                    syn::Stmt::Item(_) => {}
                    other => memoize.visit_stmt_mut(other),
                }
            }
        });
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            // A closure argument in a modifier chain (`Modifier.clickable(move || …)`) becomes
            // a callback: modifiers take `impl IntoCallback`, so it is memoized, and the
            // modifier stays `same` when its captures do.
            Expr::MethodCall(call) if is_modifier_chain(&call.receiver) => {
                self.visit_expr_mut(&mut call.receiver);
                for argument in &mut call.args {
                    if let Expr::Closure(closure) = argument {
                        self.visit_expr_closure_mut(closure);
                        *argument = memoize_closure(
                            self.depth > 0,
                            &self.scopes,
                            &self.local_macros,
                            closure,
                        );
                    } else {
                        self.visit_expr_mut(argument);
                    }
                }
            }
            // `remember`'s calculation runs once, while `remember` holds its slot: a callback
            // made there needs no memoization, and a probe there would take a slot out of order.
            Expr::Call(call) if is_remember(&call.func) => self.visit_expr_mut(&mut call.func),
            Expr::Call(call) => {
                let constructor =
                    matches!(&*call.func, Expr::Path(path) if is_callback_constructor(&path.path));
                self.visit_expr_mut(&mut call.func);
                for argument in &mut call.args {
                    if let (true, Expr::Closure(closure)) = (constructor, &mut *argument) {
                        self.visit_expr_closure_mut(closure);
                        *argument = memoize_closure(
                            self.depth > 0,
                            &self.scopes,
                            &self.local_macros,
                            closure,
                        );
                    } else {
                        self.visit_expr_mut(argument);
                    }
                }
            }
            Expr::Macro(macro_expr) => {
                self.rewrite_call_macro(&mut macro_expr.mac);
                if is_call_macro_path(&macro_expr.mac.path)
                    && let Some(path) = unqualify(&mut macro_expr.mac)
                {
                    let mac = &macro_expr.mac;
                    *expr = parse_quote! {{ use #path; #mac }};
                }
            }
            _ => visit_mut::visit_expr_mut(self, expr),
        }
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut syn::ExprClosure) {
        let mut names = Vec::new();
        for input in &closure.inputs {
            bindings(input, &mut names);
        }
        self.depth += 1;
        self.scoped(names, |memoize| memoize.visit_expr_mut(&mut closure.body));
        self.depth -= 1;
    }

    fn visit_expr_for_loop_mut(&mut self, expr: &mut syn::ExprForLoop) {
        self.visit_expr_mut(&mut expr.expr);
        let mut names = Vec::new();
        bindings(&expr.pat, &mut names);
        self.scoped(names, |memoize| memoize.visit_block_mut(&mut expr.body));
    }

    fn visit_arm_mut(&mut self, arm: &mut syn::Arm) {
        let mut names = Vec::new();
        refutable_bindings(&arm.pat, &mut names);
        self.scoped(names, |memoize| {
            if let Some((_, guard)) = &mut arm.guard {
                memoize.visit_expr_mut(guard);
            }
            memoize.visit_expr_mut(&mut arm.body);
        });
    }

    fn visit_expr_if_mut(&mut self, expr: &mut syn::ExprIf) {
        self.visit_expr_mut(&mut expr.cond);
        let mut names = Vec::new();
        condition_bindings(&expr.cond, &mut names);
        self.scoped(names, |memoize| memoize.visit_block_mut(&mut expr.then_branch));
        if let Some((_, otherwise)) = &mut expr.else_branch {
            self.visit_expr_mut(otherwise);
        }
    }

    fn visit_expr_while_mut(&mut self, expr: &mut syn::ExprWhile) {
        self.visit_expr_mut(&mut expr.cond);
        let mut names = Vec::new();
        condition_bindings(&expr.cond, &mut names);
        self.scoped(names, |memoize| memoize.visit_block_mut(&mut expr.body));
    }
}
