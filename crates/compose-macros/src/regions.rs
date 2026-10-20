//! Region groups for control flow (docs/03-composable-macro.md, "Groups for control flow").
//!
//! Each `if`/`else` branch and `match` arm containing a likely composable call gets a replace
//! group; so does each loop, as a whole. In a loop with a `continue`, each statement containing
//! a likely composable call gets its own group, since a `continue` can skip different calls in
//! different iterations. Keys are the composable's key XOR a structural index, as literals.
//!
//! Groups are guards (`let __compose_region = region(0x…);`), closed when they go out of scope,
//! so `return`, `break`, `continue` and `?` keep them balanced. `region` does nothing outside a
//! composition pass, so a group the syntactic guess puts in a handler closure is harmless.

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::visit_mut::{self, VisitMut};
use syn::{Block, Expr, Ident, Item, Label, Stmt, parse_quote};

/// Names of the runtime's composable functions that don't look like composables.
const RUNTIME_COMPOSABLES: &[&str] = &[
    "remember",
    "remember_keyed",
    "remember_observer",
    "remember_observer_keyed",
    "remember_lambda",
    "key",
    "current_recompose_scope",
];

/// PascalCase calls that are almost never composables.
const NOT_COMPOSABLES: &[&str] = &["Some", "Ok", "Err"];

fn is_composable_name(name: &str) -> bool {
    if RUNTIME_COMPOSABLES.contains(&name) {
        return true;
    }
    let mut chars = name.chars();
    chars.next().is_some_and(char::is_uppercase)
        && name.chars().any(char::is_lowercase)
        && !NOT_COMPOSABLES.contains(&name)
}

/// Whether some syntax contains a likely composable call: a call whose final path segment is
/// PascalCase or a runtime composable, a PascalCase macro, or a call of a parameter typed as a
/// function (a composable lambda).
struct Likely<'a> {
    callables: &'a [Ident],
    found: bool,
}

impl<'ast> Visit<'ast> for Likely<'_> {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*call.func {
            let path = &path.path;
            let last = &path.segments.last().expect("a path has a segment").ident;
            let callable = path.segments.len() == 1 && self.callables.contains(last);
            if callable || is_composable_name(&last.to_string()) {
                self.found = true;
                return;
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let last = &mac.path.segments.last().expect("a path has a segment").ident;
        if is_composable_name(&last.to_string()) || tokens_contain_call(mac.tokens.clone()) {
            self.found = true;
        }
    }

    fn visit_item(&mut self, _: &'ast Item) {
        // A nested item is not part of this body.
    }
}

/// In a macro's tokens, looks for `Name(` or `Name!` with a composable-looking name.
fn tokens_contain_call(tokens: TokenStream) -> bool {
    let mut previous: Option<String> = None;
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => {
                previous = Some(ident.to_string());
                continue;
            }
            TokenTree::Group(group) => {
                let call = group.delimiter() == proc_macro2::Delimiter::Parenthesis
                    && previous.as_deref().is_some_and(is_composable_name);
                if call || tokens_contain_call(group.stream()) {
                    return true;
                }
            }
            TokenTree::Punct(punct) => {
                if punct.as_char() == '!' && previous.as_deref().is_some_and(is_composable_name) {
                    return true;
                }
            }
            TokenTree::Literal(_) => {}
        }
        previous = None;
    }
    false
}

/// Whether a loop body contains a `continue` that targets the loop.
struct Continues<'a> {
    label: Option<&'a Label>,
    depth: usize,
    /// Labelled blocks inside the loop body.
    blocks: Vec<syn::Lifetime>,
    found: bool,
}

/// Standard macros that never expand to a `continue` of their own.
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
    "include_str",
    "env",
    "line",
    "file",
    "column",
    "cfg",
];

fn tokens_contain_ident(tokens: TokenStream, name: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == name,
        TokenTree::Group(group) => tokens_contain_ident(group.stream(), name),
        _ => false,
    })
}

impl<'ast> Visit<'ast> for Continues<'_> {
    fn visit_expr_continue(&mut self, expr: &'ast syn::ExprContinue) {
        match (&expr.label, self.label) {
            (None, _) if self.depth == 0 => self.found = true,
            (Some(target), Some(label)) if *target == label.name => self.found = true,
            _ => {}
        }
    }

    fn visit_expr_break(&mut self, expr: &'ast syn::ExprBreak) {
        // Breaking out of a labelled block inside the loop skips the rest of the iteration,
        // as `continue` does.
        if let Some(target) = &expr.label
            && self.blocks.contains(target)
        {
            self.found = true;
        }
        visit::visit_expr_break(self, expr);
    }

    fn visit_expr_block(&mut self, expr: &'ast syn::ExprBlock) {
        if let Some(label) = &expr.label {
            self.blocks.push(label.name.clone());
        }
        visit::visit_expr_block(self, expr);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // A macro may expand to a `continue` (an unlabelled `continue` isn't hygienic) that
        // isn't in its tokens: only well-known macros and composables' call macros are trusted.
        let name = mac.path.segments.last().map(|segment| segment.ident.to_string());
        // A PascalCase macro isn't trusted either: a user macro may be named like a composable.
        let trusted = name.as_deref().is_some_and(|name| KNOWN_MACROS.contains(&name));
        if !trusted || tokens_contain_ident(mac.tokens.clone(), "continue") {
            self.found = true;
        }
    }

    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.depth += 1;
        visit::visit_expr_for_loop(self, expr);
        self.depth -= 1;
    }

    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        self.depth += 1;
        visit::visit_expr_while(self, expr);
        self.depth -= 1;
    }

    fn visit_expr_loop(&mut self, expr: &'ast syn::ExprLoop) {
        self.depth += 1;
        visit::visit_expr_loop(self, expr);
        self.depth -= 1;
    }

    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}

    fn visit_item(&mut self, _: &'ast Item) {}
}

pub(crate) struct Regions {
    callables: Vec<Ident>,
    key: u64,
    next: u64,
}

impl Regions {
    pub fn new(callables: Vec<Ident>, key: u64) -> Regions {
        Regions { callables, key, next: 0 }
    }

    pub fn transform_block(&mut self, block: &mut Block) {
        // Local closures that make likely composable calls are callables too: calling one in a
        // branch is a likely composable call.
        let mut closures = LocalClosures { callables: &mut self.callables };
        closures.visit_block(block);
        self.visit_block_mut(block);
    }

    fn likely_block(&self, block: &Block) -> bool {
        let mut likely = Likely { callables: &self.callables, found: false };
        likely.visit_block(block);
        likely.found
    }

    fn likely_expr(&self, expr: &Expr) -> bool {
        let mut likely = Likely { callables: &self.callables, found: false };
        likely.visit_expr(expr);
        likely.found
    }

    fn likely_stmt(&self, stmt: &Stmt) -> bool {
        let mut likely = Likely { callables: &self.callables, found: false };
        likely.visit_stmt(stmt);
        likely.found
    }

    fn region(&mut self) -> Stmt {
        self.next += 1;
        let key = proc_macro2::Literal::u64_suffixed(self.key ^ self.next);
        let name = Ident::new("__compose_region", Span::mixed_site());
        parse_quote! {
            let #name = ::compose::__private::region(#key);
        }
    }

    fn wrap_block(&mut self, block: &mut Block) {
        let region = self.region();
        block.stmts.insert(0, region);
    }

    fn wrap_expr(&mut self, expr: &mut Expr) {
        let region = self.region();
        let inner = std::mem::replace(expr, Expr::Verbatim(TokenStream::new()));
        *expr = parse_quote! {{ #region #inner }};
    }

    /// In a loop with a `continue`: one group per statement containing a likely call. Plain and
    /// labelled blocks are entered, so their statements get groups of their own.
    fn wrap_statements(&mut self, body: &mut Block) {
        let stmts = std::mem::take(&mut body.stmts);
        for mut stmt in stmts {
            if let Stmt::Expr(Expr::Block(block), _) = &mut stmt {
                self.wrap_statements(&mut block.block);
                body.stmts.push(stmt);
                continue;
            }
            let has_own_group = matches!(
                stmt,
                Stmt::Expr(
                    Expr::If(_)
                        | Expr::Match(_)
                        | Expr::ForLoop(_)
                        | Expr::While(_)
                        | Expr::Loop(_),
                    _
                )
            ) || is_key_call(&stmt);
            if has_own_group || !self.likely_stmt(&stmt) {
                body.stmts.push(stmt);
                continue;
            }
            match &mut stmt {
                Stmt::Local(local) => {
                    if let Some(init) = &mut local.init {
                        self.wrap_expr(&mut init.expr);
                    }
                    body.stmts.push(stmt);
                }
                Stmt::Expr(expr, _) => {
                    self.wrap_expr(expr);
                    body.stmts.push(stmt);
                }
                Stmt::Macro(mac) => {
                    // Around the statement rather than in a block: the macro may declare
                    // bindings for the statements after it. Its `#[cfg]`s apply to the guard too.
                    let cfgs: Vec<_> = mac
                        .attrs
                        .iter()
                        .filter(|attr| attr.path().is_ident("cfg"))
                        .cloned()
                        .collect();
                    let region = self.region();
                    let name = Ident::new("__compose_region", Span::mixed_site());
                    body.stmts.push(parse_quote! { #(#cfgs)* #region });
                    body.stmts.push(stmt);
                    body.stmts.push(parse_quote! { #(#cfgs)* ::core::mem::drop(#name); });
                }
                Stmt::Item(_) => body.stmts.push(stmt),
            }
        }
    }

    fn visit_if(&mut self, expr: &mut syn::ExprIf) {
        self.visit_expr_mut(&mut expr.cond);
        self.visit_block_mut(&mut expr.then_branch);
        if self.likely_block(&expr.then_branch) {
            self.wrap_block(&mut expr.then_branch);
        }
        if let Some((_, otherwise)) = &mut expr.else_branch {
            match &mut **otherwise {
                Expr::Block(block) => {
                    self.visit_block_mut(&mut block.block);
                    if self.likely_block(&block.block) {
                        self.wrap_block(&mut block.block);
                    }
                }
                other => self.visit_expr_mut(other),
            }
        }
    }

    fn visit_loop_body(&mut self, label: Option<&Label>, body: &mut Block) {
        self.visit_block_mut(body);
        let mut continues = Continues { label, depth: 0, blocks: Vec::new(), found: false };
        continues.visit_block(body);
        if continues.found {
            self.wrap_statements(body);
        }
    }
}

/// `key(…)`: a movable group already, and it must stay outermost in its loop body so items
/// match by key across the whole loop.
fn is_key_call(stmt: &Stmt) -> bool {
    fn key(expr: &Expr) -> bool {
        let Expr::Call(call) = expr else { return false };
        matches!(&*call.func, Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "key"))
    }
    match stmt {
        Stmt::Expr(Expr::Assign(assign), _) => key(&assign.right),
        Stmt::Expr(expr, _) => key(expr),
        Stmt::Local(local) => local.init.as_ref().is_some_and(|init| key(&init.expr)),
        _ => false,
    }
}

/// Finds `let name = |…| …` closures making likely composable calls.
struct LocalClosures<'a> {
    callables: &'a mut Vec<Ident>,
}

impl<'ast> Visit<'ast> for LocalClosures<'_> {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (syn::Pat::Ident(name), Some(init)) = (&local.pat, &local.init)
            && let Expr::Closure(closure) = &*init.expr
        {
            let mut likely = Likely { callables: self.callables, found: false };
            likely.visit_expr(&closure.body);
            if likely.found {
                self.callables.push(name.ident.clone());
            }
        }
        visit::visit_local(self, local);
    }

    fn visit_item(&mut self, _: &'ast Item) {}
}

impl VisitMut for Regions {
    fn visit_item_mut(&mut self, _: &mut Item) {
        // A nested item is not part of this body; a nested composable has its own attribute.
    }

    fn visit_local_mut(&mut self, local: &mut syn::Local) {
        visit_mut::visit_local_mut(self, local);
        // `let … else { … }`: the else block is a branch.
        if let Some(init) = &mut local.init
            && let Some((_, otherwise)) = &mut init.diverge
            && self.likely_expr(otherwise)
        {
            self.wrap_expr(otherwise);
        }
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            Expr::If(if_expr) => self.visit_if(if_expr),
            Expr::Match(match_expr) => {
                self.visit_expr_mut(&mut match_expr.expr);
                for arm in &mut match_expr.arms {
                    if let Some((_, guard)) = &mut arm.guard {
                        self.visit_expr_mut(guard);
                    }
                    self.visit_expr_mut(&mut arm.body);
                    if self.likely_expr(&arm.body) {
                        self.wrap_expr(&mut arm.body);
                    }
                }
            }
            Expr::ForLoop(for_loop) => {
                self.visit_expr_mut(&mut for_loop.expr);
                let label = for_loop.label.clone();
                self.visit_loop_body(label.as_ref(), &mut for_loop.body);
                if self.likely_expr(expr) {
                    self.wrap_expr(expr);
                }
            }
            Expr::While(while_loop) => {
                self.visit_expr_mut(&mut while_loop.cond);
                let label = while_loop.label.clone();
                self.visit_loop_body(label.as_ref(), &mut while_loop.body);
                if self.likely_expr(expr) {
                    self.wrap_expr(expr);
                }
            }
            Expr::Loop(loop_expr) => {
                let label = loop_expr.label.clone();
                self.visit_loop_body(label.as_ref(), &mut loop_expr.body);
                if self.likely_expr(expr) {
                    self.wrap_expr(expr);
                }
            }
            // Futures run outside composition, and a region guard held across an `.await` would
            // make them `!Send`; `const` blocks can't call the runtime.
            Expr::Async(_) | Expr::Const(_) => {}
            Expr::Closure(closure) if closure.asyncness.is_some() => {}
            // A closure making likely calls is a region of its own wherever it runs, so a call
            // it makes conditionally (`flag.then(|| Row(1))`) doesn't lend its state to the next.
            Expr::Closure(closure) => {
                self.visit_expr_mut(&mut closure.body);
                if self.likely_expr(&closure.body) {
                    self.wrap_expr(&mut closure.body);
                }
            }
            _ => visit_mut::visit_expr_mut(self, expr),
        }
    }
}
