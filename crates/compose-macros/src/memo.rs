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
use syn::{Expr, Pat, Token};

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
    "cfg",
    "line",
    "column",
    "file",
    "env",
    "option_env",
    "module_path",
    "include_str",
    "include_bytes",
    "compile_error",
    "write",
];

fn is_call_macro(name: &str) -> bool {
    crate::regions::is_pascal_case(name)
}

/// The modifiers whose closure arguments become callbacks (`impl IntoCallback`); others
/// (`semantics`, a user's `then_if(cond, |m| …)`) take closures they run at once.
const CALLBACK_MODIFIERS: &[&str] =
    &["clickable", "clickable_with", "toggleable", "on_focus_changed", "on_key_event"];

/// Whether `receiver` is a modifier chain: method calls on the path `Modifier`.
fn is_modifier_chain(receiver: &Expr, scopes: &Scopes) -> bool {
    match receiver {
        Expr::MethodCall(call) => is_modifier_chain(&call.receiver, scopes),
        Expr::Path(path) => {
            path.qself.is_none()
                && path.path.segments.last().is_some_and(|segment| {
                    // `Modifier.…`, a composable's `modifier` parameter, or a local holding one.
                    segment.ident == "Modifier"
                        || segment.ident == "modifier"
                        || (path.path.segments.len() == 1
                            && scopes.is_modifier(&segment.ident.to_string()))
                })
        }
        Expr::Paren(paren) => is_modifier_chain(&paren.expr, scopes),
        _ => false,
    }
}

/// Whether a `let`'s value is a modifier: a modifier chain, or a branch whose arms all are.
fn is_modifier_value(value: &Expr, scopes: &Scopes) -> bool {
    let tail = |block: &syn::Block| match block.stmts.last() {
        Some(syn::Stmt::Expr(value, None)) => is_modifier_value(value, scopes),
        _ => false,
    };
    match value {
        Expr::If(branch) => {
            tail(&branch.then_branch)
                && branch
                    .else_branch
                    .as_ref()
                    .is_some_and(|(_, otherwise)| is_modifier_value(otherwise, scopes))
        }
        Expr::Block(block) => tail(&block.block),
        Expr::Match(choice) => {
            !choice.arms.is_empty()
                && choice.arms.iter().all(|arm| is_modifier_value(&arm.body, scopes))
        }
        other => is_modifier_chain(other, scopes),
    }
}

/// `Modifier`, as a type.
fn is_modifier_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(path) if path.qself.is_none() && path.path.segments.last().is_some_and(|segment| segment.ident == "Modifier" && segment.arguments.is_none()))
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

/// Whether `block` is a probe this pass wrote (it starts by importing the probe traits): a
/// callback value, not a content block.
pub(crate) fn is_probe_block(block: &syn::Block) -> bool {
    matches!(block.stmts.first(), Some(syn::Stmt::Item(syn::Item::Use(item)))
        if item.to_token_stream().to_string().contains("KeyedProbe"))
}

/// A lambda's site id: a hash of its code, as a literal (`lambda_probe`). Two lambdas with the
/// same code and keys behave the same, so they may share an id.
fn site_of(code: &TokenStream) -> proc_macro2::Literal {
    proc_macro2::Literal::u64_suffixed(crate::composable::key_of(&code.to_string()))
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
    arity.filter(|arity| *arity <= 8).map(|arity| format_ident!("resolve_{}", arity))
}

/// The runtime function wrapping a lambda of `arity` arguments in a `Callback`, as `resolver`.
pub(crate) fn wrapper(arity: usize) -> Ident {
    if arity <= 8 { format_ident!("callback_{}", arity) } else { format_ident!("callback") }
}

/// A lookup of the lambda remembered at this position, keyed with `keys`: the kept callback if
/// they are all `same` as last pass's, else `make` stored.
fn probe_with(keys: Vec<TokenStream>, make: TokenStream, arity: Option<usize>) -> TokenStream {
    let site = site_of(&make);
    let keys = key_tuple(keys);
    let resolve = match resolver(arity) {
        Some(resolver) => {
            quote! { ::compose::__private::#resolver(::compose::__private::lambda_probe(#site, #keys), #make) }
        }
        None => quote! { ::compose::__private::lambda_probe(#site, #keys).resolve(#make) },
    };
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, StaticProbe as _, UnkeyedProbe as _};
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
        .map(|capture| quote! { (&&&::compose::__private::ArgProbe(&#capture)).arg_key() })
        .collect();
    if unknown {
        keys.push(quote! { ::compose::__private::Unkeyed::CHANGED });
    }
    if !nested {
        return probe_with(keys, closure, arity);
    }
    let site = site_of(&closure);
    let keys = key_tuple(keys);
    let resolver = match arity.filter(|arity| *arity <= 8) {
        Some(arity) => format_ident!("resolve_lazy_{}", arity),
        None => format_ident!("resolve_lazy"),
    };
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, StaticProbe as _, UnkeyedProbe as _};
        ::compose::__private::#resolver(::compose::__private::lambda_probe(#site, #keys), || {
            #(let #captures = ::core::clone::Clone::clone(&#captures);)*
            #closure
        })
    }}
}

/// A callback given as a value (`on_click = on_remove`, a default): memoized on the value
/// itself, so the same callback, function or capture-less closure passes the same `Callback`.
pub(crate) fn probe_value(value: TokenStream) -> TokenStream {
    let name = Ident::new("__compose_value", Span::mixed_site());
    let key = quote! { (&&&::compose::__private::ArgProbe(#name)).arg_key() };
    let site = site_of(&value);
    let keys = key_tuple(vec![key]);
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, StaticProbe as _, UnkeyedProbe as _};
        let #name = &(#value);
        ::compose::__private::lambda_probe(#site, #keys).resolve_cloned(#name)
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

/// The names a macro's arguments may bind, as a user's PascalCase macro could: in each
/// argument, the names before a lone `=` (`current = …`, `(low, high) = …`), or the whole
/// argument when it is a name or a tuple of names leading the arguments (`doubled, n * 2`, as
/// `let`'s pattern leads). Values after `=`, expressions, and a name after a value
/// (`Clickable!("save", save)`, a function given positionally) are uses, not bindings.
fn unbound_names(tokens: TokenStream) -> Vec<String> {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while", "_",
    ];
    fn names_in(tokens: &[TokenTree], out: &mut Vec<String>) {
        for token in tokens {
            match token {
                TokenTree::Ident(ident) => {
                    let name = ident.to_string();
                    if !crate::regions::is_pascal_case(&name) && !KEYWORDS.contains(&name.as_str())
                    {
                        out.push(name);
                    }
                }
                TokenTree::Group(group) if group.delimiter() != proc_macro2::Delimiter::Brace => {
                    let inner: Vec<TokenTree> = group.stream().into_iter().collect();
                    names_in(&inner, out);
                }
                _ => {}
            }
        }
    }
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut names = Vec::new();
    let mut leading = true;
    for segment in
        tokens.split(|token| matches!(token, TokenTree::Punct(punct) if punct.as_char() == ','))
    {
        // A lone `=`: not `==`, `!=`, `<=`, `>=`, `=>`.
        let equals = segment.iter().enumerate().position(|(index, token)| {
            matches!(token, TokenTree::Punct(punct) if punct.as_char() == '=' && punct.spacing() == proc_macro2::Spacing::Alone)
                && !matches!(index.checked_sub(1).and_then(|previous| segment.get(previous)), Some(TokenTree::Punct(previous)) if previous.spacing() == proc_macro2::Spacing::Joint)
        });
        match equals {
            Some(equals) => names_in(&segment[..equals], &mut names),
            None => match segment {
                [TokenTree::Ident(_)] if leading => names_in(segment, &mut names),
                [TokenTree::Group(group)]
                    if leading && group.delimiter() == proc_macro2::Delimiter::Parenthesis =>
                {
                    names_in(segment, &mut names)
                }
                _ => leading = false,
            },
        }
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

    /// Records that the local `name` just bound holds a modifier (`let base = Modifier.…;`).
    fn bind_modifier(&mut self, name: &str) {
        self.0.last_mut().expect("a scope").push((format!("{MODIFIER}{name}"), false));
    }

    /// Whether the innermost binding of `name` holds a modifier.
    fn is_modifier(&self, name: &str) -> bool {
        self.0
            .iter()
            .rev()
            .find_map(|scope| {
                scope.iter().rev().find_map(|(bound, _)| {
                    if bound == name {
                        Some(false)
                    } else {
                        bound.strip_prefix(MODIFIER).filter(|bound| *bound == name).map(|_| true)
                    }
                })
            })
            .unwrap_or(false)
    }
}

/// Marks, in a scope, that the binding before it holds a modifier.
const MODIFIER: char = '\u{2}';

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
            // A name a macro may have bound: the lambda is always changed, and the name is
            // still captured (cloned into nested lambdas, D83).
            Some(true) => {
                self.unknown = true;
                if self.seen.insert(name) {
                    self.found.push(ident.clone());
                }
            }
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
                    // The string's value, escapes resolved (`"{\x63ount}"` names `count`).
                    let value = syn::parse_str::<syn::LitStr>(&literal.to_string())
                        .map(|text| text.value());
                    for name in value.map(|value| format_names(&value)).unwrap_or_default() {
                        // The literal's span: the name resolves where the string was written.
                        self.use_name(&Ident::new(&name, literal.span()));
                    }
                }
                TokenTree::Punct(punct) => {
                    // A macro called inside a macro's tokens.
                    // `name!`, not `name != …`.
                    if punct.as_char() == '!'
                        && punct.spacing() == proc_macro2::Spacing::Alone
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
pub(crate) fn format_names(literal: &str) -> Vec<String> {
    // An identifier, as Rust reads one (`²` is alphanumeric but not an identifier's).
    let is_name = |name: &str| {
        !name.is_empty() && !name.starts_with("r#") && syn::parse_str::<syn::Ident>(name).is_ok()
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
                // After the `0` flag (`{:0width$}`), the digits aren't the name's.
                let name = word.trim_start_matches(|c: char| c.is_ascii_digit());
                if c == '$' && is_name(name) {
                    names.push(name.to_string());
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
        // A call macro's arguments are expressions: visited as such, so names bound inside its
        // lambdas are scoped, and a named argument's name (`modifier = …`) isn't a use (D98).
        if name.as_deref().is_some_and(is_call_macro) {
            let parser = Punctuated::<crate::call::Argument, Token![,]>::parse_terminated;
            let (_, tokens) = crate::call::split_path_prefix(mac.tokens.clone());
            if let Ok(arguments) = syn::parse::Parser::parse2(parser, tokens) {
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

    /// A string literal may be a format string (`Text!("Count: {count}")`).
    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        for name in format_names(&literal.value()) {
            self.use_name(&Ident::new(&name, literal.span()));
        }
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

    /// In a let chain (`let Some(x) = a && f(x)`), the names bound by the left operand are in
    /// scope in the right one.
    fn visit_expr_binary(&mut self, expr: &'ast syn::ExprBinary) {
        if !matches!(expr.op, syn::BinOp::And(_)) {
            return visit::visit_expr_binary(self, expr);
        }
        self.visit_expr(&expr.left);
        let mut names = Vec::new();
        condition_bindings(&expr.left, &mut names);
        self.scoped(names, |free| free.visit_expr(&expr.right));
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
            /// `macro_rules! Name` inside a macro's tokens (a content block of a call macro).
            fn visit_macro(&mut self, mac: &'ast syn::Macro) {
                fn scan(tokens: TokenStream, out: &mut Vec<String>) {
                    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
                    for (index, token) in tokens.iter().enumerate() {
                        match token {
                            TokenTree::Ident(ident) if ident == "macro_rules" => {
                                if let (Some(TokenTree::Punct(bang)), Some(TokenTree::Ident(name))) =
                                    (tokens.get(index + 1), tokens.get(index + 2))
                                    && bang.as_char() == '!'
                                {
                                    out.push(name.to_string());
                                }
                            }
                            TokenTree::Group(group) => scan(group.stream(), out),
                            _ => {}
                        }
                    }
                }
                scan(mac.tokens.clone(), &mut self.0);
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
            // `Some(lambda)` for an optional callback: the lambda marked.
            Expr::Call(call)
                if crate::call::is_some(&call.func)
                    && call.args.len() == 1
                    && matches!(call.args[0], Expr::Closure(_)) =>
            {
                let function = &call.func;
                let mut lambda = TokenStream::new();
                self.mark(&mut call.args[0], &mut lambda);
                tokens.extend(quote! { #function(#lambda) });
            }
            other => {
                self.visit_expr_mut(other);
                other.to_tokens(tokens);
            }
        }
    }

    /// A composable's call macro: its closures and blocks marked; invoked by path
    /// (`ui::Label!(…)`), the path passed first (`@path (ui::Label)`), as its expansion would
    /// otherwise call the bare name, whatever `Label` is where it's used.
    fn rewrite_call_macro(&mut self, mac: &mut syn::Macro) {
        let name = mac.path.segments.last().map(|segment| segment.ident.to_string());
        if !name.as_deref().is_some_and(is_call_macro) {
            return;
        }
        self.rewrite_call_macro_arguments(mac);
        if mac.path.segments.len() > 1 {
            let path = &mac.path;
            let tokens = &mac.tokens;
            mac.tokens = quote! { @path (#path) #tokens };
        }
    }

    fn rewrite_call_macro_arguments(&mut self, mac: &mut syn::Macro) {
        if mac.tokens.is_empty() {
            return;
        }
        // A PascalCase macro may be the user's, binding names it's given (`Bind!(current = …)`,
        // `Scoped!(doubled, n * 2, { … })`): a new name in its arguments, not a function, path or
        // field, is tainted for the lambdas in and after it. Names of locals keep their meaning.
        let new_names = unbound_names(mac.tokens.clone())
            .into_iter()
            .filter(|name| !self.scopes.contains(name))
            .collect();
        self.scopes.taint(new_names);
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
            let statements = std::mem::take(&mut block.stmts);
            for mut stmt in statements {
                match &mut stmt {
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
                        // `let base = Modifier.…;` or `let base: Modifier = …;`: a modifier
                        // chain on `base` memoizes its callbacks too.
                        let (pattern, typed) = match &local.pat {
                            syn::Pat::Type(typed) => (&*typed.pat, is_modifier_type(&typed.ty)),
                            pattern => (pattern, false),
                        };
                        if let syn::Pat::Ident(ident) = pattern
                            && ident.subpat.is_none()
                            && (typed
                                || local.init.as_ref().is_some_and(|init| {
                                    init.diverge.is_none()
                                        && is_modifier_value(&init.expr, &memoize.scopes)
                                }))
                        {
                            memoize.scopes.bind_modifier(&ident.ident.to_string());
                        }
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
                            memoize.rewrite_call_macro(&mut statement.mac);
                        } else if !is_known_macro(&statement.mac.path) {
                            // It may bind locals the walk can't see: a lambda naming a new
                            // identifier it was given is always changed. A local it was given
                            // keeps its key, which names whatever binding is in scope where
                            // the lambda is (the macro's, if it shadowed it).
                            let mut names = Vec::new();
                            token_idents(statement.mac.tokens.clone(), &mut names);
                            names.retain(|name| !memoize.scopes.contains(name));
                            memoize.scopes.taint(names);
                        }
                    }
                    syn::Stmt::Item(_) => {}
                    other => memoize.visit_stmt_mut(other),
                }
                block.stmts.push(stmt);
            }
        });
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            // A closure argument in a modifier chain (`Modifier.clickable(move || …)`) becomes
            // a callback: modifiers take `impl IntoCallback`, so it is memoized, and the
            // modifier stays `same` when its captures do.
            Expr::MethodCall(call)
                if is_modifier_chain(&call.receiver, &self.scopes)
                    && CALLBACK_MODIFIERS.contains(&call.method.to_string().as_str()) =>
            {
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
            // `callback(closure)`: the memoized closure replaces the call, so its parameters take
            // their types from where the callback goes.
            Expr::Call(call)
                if call.args.len() == 1
                    && matches!(&call.args[0], Expr::Closure(_))
                    && matches!(&*call.func, Expr::Path(path) if is_callback_constructor(&path.path)) =>
            {
                let Some(Expr::Closure(closure)) = call.args.first_mut() else { unreachable!() };
                self.visit_expr_closure_mut(closure);
                *expr = memoize_closure(self.depth > 0, &self.scopes, &self.local_macros, closure);
            }
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
            Expr::Macro(macro_expr) => self.rewrite_call_macro(&mut macro_expr.mac),
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

    /// A loop body runs again: the lambdas in it are nested, built from clones (D83).
    fn visit_expr_for_loop_mut(&mut self, expr: &mut syn::ExprForLoop) {
        self.visit_expr_mut(&mut expr.expr);
        let mut names = Vec::new();
        bindings(&expr.pat, &mut names);
        self.depth += 1;
        self.scoped(names, |memoize| memoize.visit_block_mut(&mut expr.body));
        self.depth -= 1;
    }

    fn visit_expr_loop_mut(&mut self, expr: &mut syn::ExprLoop) {
        self.depth += 1;
        self.visit_block_mut(&mut expr.body);
        self.depth -= 1;
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
        self.depth += 1;
        self.scoped(names, |memoize| memoize.visit_block_mut(&mut expr.body));
        self.depth -= 1;
    }

    /// In a let chain, the names bound by the left operand are in scope in the right one.
    fn visit_expr_binary_mut(&mut self, expr: &mut syn::ExprBinary) {
        if !matches!(expr.op, syn::BinOp::And(_)) {
            return visit_mut::visit_expr_binary_mut(self, expr);
        }
        self.visit_expr_mut(&mut expr.left);
        let mut names = Vec::new();
        condition_bindings(&expr.left, &mut names);
        self.scoped(names, |memoize| memoize.visit_expr_mut(&mut expr.right));
    }
}
