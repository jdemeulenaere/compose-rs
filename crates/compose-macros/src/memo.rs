//! Lambda memoization (docs/03-composable-macro.md, "Lambdas: memoized on their captures").
//!
//! A closure passed to a composable call becomes a `Callback` remembered at its position, and
//! replaced only when one of its captures is not `same` as last pass: the callee sees the same
//! callback and can skip. The captures are found syntactically: the identifiers in the closure
//! that name locals of the enclosing function visible at that point, tracked through `let`,
//! parameters, `for`, `match` arms, `if let` and closure parameters. A real capture must never be
//! missed, so identifiers in macro arguments and inline format arguments (`"{count}"`) count, and
//! a closure calling an unknown macro counts as always changed.
//!
//! The macro sees syntax, not types, so it only rewrites closures it knows become callbacks:
//! - in a composable's call macro, every closure and the trailing block are marked
//!   `@memoize(captures) …`, and the call macro memoizes those bound to callback parameters (it
//!   knows its parameters, and the trailing block's arity);
//! - elsewhere, a closure wrapped as `callback(…)` or `Callback::new(…)` is memoized.
//!
//! Closures passed to other functions (`map`, `update`, `key`, a generic `F: Fn()` parameter)
//! keep their types.

use std::collections::HashSet;

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
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
];

fn is_call_macro(name: &str) -> bool {
    crate::regions::is_pascal_case(name)
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

/// The memoized form of `closure`: a probe of the lambda remembered at this position.
pub(crate) fn probe(captures: &[Ident], unknown: bool, closure: TokenStream) -> TokenStream {
    let probe = Ident::new("__compose_probe", Span::mixed_site());
    let callback = Ident::new("__compose_callback", Span::mixed_site());
    let pending = Ident::new("__compose_pending", Span::mixed_site());
    let keys = captures.iter().map(|capture| {
        quote! { (&&::compose::__private::ArgProbe(&#capture)).arg_key() }
    });
    let unknown = unknown.then(|| quote! { ::compose::__private::Unkeyed, });
    quote! {{
        #[allow(unused_imports)]
        use ::compose::__private::{KeyedProbe as _, UnkeyedProbe as _};
        let #probe = ::compose::__private::lambda_probe((#(#keys,)* #unknown));
        match #probe {
            ::compose::__private::LambdaProbe::Hit(#callback) => #callback,
            ::compose::__private::LambdaProbe::Miss(#pending) => #pending.store(#closure),
        }
    }}
}

/// The names a pattern binds.
pub(crate) fn pattern_names(pattern: &Pat, out: &mut Vec<String>) {
    bindings(pattern, out)
}

/// The names a pattern binds. A PascalCase identifier is taken for a constant or a unit variant.
fn bindings(pattern: &Pat, out: &mut Vec<String>) {
    struct Bindings<'a>(&'a mut Vec<String>);
    impl<'ast> Visit<'ast> for Bindings<'_> {
        fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
            let name = pattern.ident.to_string();
            if !crate::regions::is_pascal_case(&name) {
                self.0.push(name);
            }
            visit::visit_pat_ident(self, pattern);
        }
        fn visit_expr(&mut self, _: &'ast Expr) {
            // Range bounds and constants in patterns bind nothing.
        }
    }
    Bindings(out).visit_pat(pattern);
}

/// Names bound by `let` conditions (`if let`, `while let`, let chains).
fn condition_bindings(condition: &Expr, out: &mut Vec<String>) {
    match condition {
        Expr::Let(binding) => bindings(&binding.pat, out),
        Expr::Binary(binary) if matches!(binary.op, syn::BinOp::And(_)) => {
            condition_bindings(&binary.left, out);
            condition_bindings(&binary.right, out);
        }
        Expr::Paren(paren) => condition_bindings(&paren.expr, out),
        _ => {}
    }
}

/// Scopes of local names, innermost last.
#[derive(Clone, Default)]
pub(crate) struct Scopes(Vec<Vec<String>>);

impl Scopes {
    fn push(&mut self, names: Vec<String>) {
        self.0.push(names);
    }

    fn pop(&mut self) {
        self.0.pop();
    }

    fn bind(&mut self, names: Vec<String>) {
        self.0.last_mut().expect("a scope").extend(names);
    }

    fn contains(&self, name: &str) -> bool {
        self.0.iter().any(|scope| scope.iter().any(|bound| bound == name))
    }
}

/// The captures of a closure or block: free identifiers naming locals of `outer`.
struct Free<'a> {
    outer: &'a Scopes,
    inner: Scopes,
    found: Vec<Ident>,
    seen: HashSet<String>,
    unknown: bool,
}

impl Free<'_> {
    fn use_name(&mut self, ident: &Ident) {
        let name = ident.to_string();
        if !self.inner.contains(&name) && self.outer.contains(&name) && self.seen.insert(name) {
            self.found.push(ident.clone());
        }
    }

    fn scan_tokens(&mut self, tokens: TokenStream) {
        for token in tokens {
            match token {
                TokenTree::Ident(ident) => self.use_name(&ident),
                TokenTree::Group(group) => self.scan_tokens(group.stream()),
                TokenTree::Literal(literal) => {
                    for name in format_names(&literal.to_string()) {
                        self.use_name(&Ident::new(&name, Span::call_site()));
                    }
                }
                TokenTree::Punct(_) => {}
            }
        }
    }

    fn scoped(&mut self, names: Vec<String>, f: impl FnOnce(&mut Self)) {
        self.inner.push(names);
        f(self);
        self.inner.pop();
    }
}

/// Inline format arguments in a string literal: `"{count}"`, `"{value:>8}"`.
fn format_names(literal: &str) -> Vec<String> {
    if !literal.starts_with('"') && !literal.starts_with('r') {
        return Vec::new();
    }
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
        let name: String =
            chars.by_ref().take_while(|c| *c != '}' && *c != ':').collect::<String>();
        let name = name.trim();
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            names.push(name.to_string());
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
        let known = name.as_deref().is_some_and(|name| {
            KNOWN_MACROS.contains(&name) || crate::regions::is_pascal_case(name)
        });
        if !known {
            self.unknown = true;
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
                        bindings(&local.pat, &mut names);
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
        bindings(&arm.pat, &mut names);
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
fn captures_of(outer: &Scopes, visit: impl FnOnce(&mut Free<'_>)) -> (Vec<Ident>, bool) {
    let mut free = Free {
        outer,
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
fn memoize_closure(scopes: &Scopes, closure: &mut syn::ExprClosure) -> Expr {
    let (captures, unknown) = captures_of(scopes, |free| free.visit_expr_closure(closure));
    closure.capture.get_or_insert_with(Default::default);
    let tokens = probe(&captures, unknown, closure.to_token_stream());
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
}

impl Memoize {
    pub fn new(parameters: Vec<String>) -> Memoize {
        let mut scopes = Scopes::default();
        scopes.push(parameters);
        Memoize { scopes }
    }

    /// A call macro argument: a closure (or the trailing block) marked with its captures,
    /// anything else visited.
    fn mark(&mut self, expression: &mut Expr, trailing: bool, tokens: &mut TokenStream) {
        match expression {
            Expr::Closure(closure) => {
                // Inner calls first, with the closure's parameters in scope.
                self.visit_expr_closure_mut(closure);
                let (captures, unknown) =
                    captures_of(&self.scopes, |free| free.visit_expr_closure(closure));
                Marked::new(captures, unknown, MarkedBody::Closure(closure.clone()))
                    .to_tokens(tokens);
            }
            Expr::Block(block) if trailing && block.label.is_none() && block.attrs.is_empty() => {
                self.visit_block_mut(&mut block.block);
                let (captures, unknown) =
                    captures_of(&self.scopes, |free| free.visit_block(&block.block));
                Marked::new(captures, unknown, MarkedBody::Block(block.block.clone()))
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
        let last = arguments.len().wrapping_sub(1);
        let mut tokens = TokenStream::new();
        for (index, argument) in arguments.iter_mut().enumerate() {
            if index > 0 {
                tokens.extend(quote!(,));
            }
            match argument {
                MacroArgument::Positional(expression) => {
                    self.mark(expression, index == last, &mut tokens);
                }
                MacroArgument::Named(name, equals, expression) => {
                    name.to_tokens(&mut tokens);
                    equals.to_tokens(&mut tokens);
                    self.mark(expression, false, &mut tokens);
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
                        bindings(&local.pat, &mut names);
                        memoize.scopes.bind(names);
                    }
                    syn::Stmt::Macro(statement) => memoize.rewrite_call_macro(&mut statement.mac),
                    syn::Stmt::Item(_) => {}
                    other => memoize.visit_stmt_mut(other),
                }
            }
        });
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Call(call) => {
                let constructor =
                    matches!(&*call.func, Expr::Path(path) if is_callback_constructor(&path.path));
                self.visit_expr_mut(&mut call.func);
                for argument in &mut call.args {
                    if let (true, Expr::Closure(closure)) = (constructor, &mut *argument) {
                        self.visit_expr_closure_mut(closure);
                        *argument = memoize_closure(&self.scopes, closure);
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
        self.scoped(names, |memoize| memoize.visit_expr_mut(&mut closure.body));
    }

    fn visit_expr_for_loop_mut(&mut self, expr: &mut syn::ExprForLoop) {
        self.visit_expr_mut(&mut expr.expr);
        let mut names = Vec::new();
        bindings(&expr.pat, &mut names);
        self.scoped(names, |memoize| memoize.visit_block_mut(&mut expr.body));
    }

    fn visit_arm_mut(&mut self, arm: &mut syn::Arm) {
        let mut names = Vec::new();
        bindings(&arm.pat, &mut names);
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
