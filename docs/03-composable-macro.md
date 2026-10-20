# 3. The `#[composable]` transform

Compose's compiler plugin rewrites every `@Composable` function. Here a proc macro does the same job. A proc macro sees tokens, not types, so the design follows one rule: **the macro decides what syntax allows, and trait resolution decides what types require.** Where the macro can't know something, it emits code that lets rustc pick the behaviour through traits. A wrong guess may cost a skip; it must never cost correctness.

The macro does **one syntactic walk** over the function body. Expansion is linear in the body's size, and generated code calls shared, non-generic runtime functions wherever possible, to keep compile times down.

## What a definition becomes

```rust
#[composable]
fn Greeting(name: SharedString, count: i32) {
    Text!("Hello {name}: {count}");
}
```

expands to (`crates/compose-macros/tests/expand/` holds the golden expansions):

```rust
#[allow(non_snake_case)]
fn Greeting(name: SharedString, count: i32) {
    let __composer = ::compose::__private::current();             // thread-local composer
    // A guard: the restart group ends when it drops, so early returns stay balanced. Its restart
    // function is a plain fn pointer, no allocation: it re-runs Greeting with the kept arguments.
    let __compose_group = ::compose::__private::RestartGroup::start(__composer, 0x2f6b…_u64,
        |kept: &dyn Any| {
            let (name, count) = ::compose::__private::kept::<(SharedString, i32)>(kept);
            Greeting(name, count)
        });
    // Kept arguments live in one typed slot: (SharedString, i32). The tuple is built (cloned)
    // only when an argument differs.
    if !__composer.update_kept(
        |kept: &(SharedString, i32)| !ComposeArg::same(&kept.0, &name) || !ComposeArg::same(&kept.1, &count),
        || (name.clone(), count.clone()),
    ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    Text(format_shared!("Hello {name}: {count}"));                 // the body, transformed
}
```

The pieces:

- **Group key**: a hash of the declaration, computed by the macro and emitted as a literal: the crate name, the source file (relative to the package) and the function's name. It doesn't depend on line numbers or formatting, so keys and test dumps stay stable across unrelated edits, and it is the same on every machine. A literal rather than a `const` item: each `const` costs the compiler a body to check and evaluate, about a third of a millisecond per composable.
- **Kept arguments**: one slot holds a tuple of the arguments. It is allocated once and updated in place when an argument changes. Restart reads it back.
- **Restart function**: a non-capturing closure, which coerces to a plain `fn` pointer. Registering it allocates nothing.
- **Groups are guards.** Restart, replace and region groups end when their guard drops, so `return`, `break`, `continue` and `?` in a body keep them balanced without rewriting jumps. While a panic unwinds, guards don't end their groups: the pass is poisoned anyway ([04](04-runtime.md)).
- **Paths.** Generated code calls `::compose::__private`. A crate using the macro without the `compose` facade (the runtime's own tests, the toolkit crates) aliases its runtime: `extern crate compose_runtime as compose;`.
- **Free functions only.** A composable can't be a method: its restart function must name it, and a `self` receiver is a borrow. Parameters written as patterns (`(x, y): (i32, i32)`) are renamed and destructured at the start of the body.
- **The current composer** is a thread-local. Composables stay ordinary functions, callable from any composable without the caller passing anything.
- **No `$changed` bits from the caller.** In Compose, callers pass static/same/different hints per argument. Here each callee compares its arguments with its kept tuple, which costs one cheap comparison per argument. This keeps calls plain and costs little, since `same` is identity or a small value compare (below). Caller hints can be added later if profiles ask for them.

## Strong skipping through `ComposeArg`

There is no stability inference, as with Compose's strong skipping. Every restartable composable can skip, and each argument is compared through a trait:

```rust
pub trait ComposeArg: Clone + 'static {
    /// Cheap: identity first, then contents only for small values; never a deep traversal.
    fn same(&self, other: &Self) -> bool;
}
```

**The rule: identity first, then contents only where that's cheap.**
- Shared data (`Rc`, `Arc`, state handles, callbacks) compares **by identity only**.
- Small values (numbers, short strings, small structs and enums) compare **by contents**.
- Collections are **never** compared element by element.

This combines the strengths of the two pure approaches:

| | Identity only | `PartialEq` everywhere | This rule |
|---|---|---|---|
| Cost per comparison | O(1), always | proportional to the data, hidden on every recomposition of the parent | O(1) or a small bounded compare |
| Kept copy in the slot | a reference (`Rc` clone) | a full clone of the argument | a reference, or a copy of a small value |
| Rebuilt but equal data (`Text(format!(…))`, a struct literal) | recomposes needlessly | skips | skips for small values; for shared data the caller keeps the `Rc` (or uses state) |
| Large collections | must be wrapped in `Rc` | silently O(n) per pass | must be wrapped in `Rc`, by design |

Compose itself is a hybrid of this kind: stable types compare with `equals`, everything else by identity under strong skipping.

| Type | `same` | Notes |
|---|---|---|
| integers, floats, `bool`, `char`, `Dp`, `Color`, unit-like enums | value | bitwise-cheap |
| `&'static str`, `SharedString` (`Rc<str>` or static) | pointer and length, then value if short | `String` itself is accepted and compared by value: UI strings are short |
| `Rc<T>`, `Arc<T>` | `ptr_eq` | **the way to pass large shared data**: identity, as JVM references are |
| `MutableState<T>`, `State<T>`, `OwnedState` handles | handle identity | see [05](05-state.md) |
| `Callback<A>` (memoized lambdas, below) | `Rc::ptr_eq` | |
| `Modifier` | element by element, each element's own cheap `same` | chains are short |
| `Option<T>`, tuples, arrays of `ComposeArg` | component-wise | |
| `Vec<T>`, `HashMap`, `String`-heavy structs | **not implemented** | deep comparisons are what strong skipping avoids; wrap them in `Rc` |
| user structs and enums | `#[derive(ComposeArg)]`: field-wise `same`, so `Rc` fields by identity and small fields by value | the Rust analogue of a stable data class; a struct holding a `Vec` field doesn't derive |

A composable whose parameter type doesn't implement `ComposeArg` fails to compile, with a note naming the parameter and suggesting `Rc<…>` or `#[composable(non_restartable)]`.

### Borrowed parameters

A restartable composable must keep its arguments across passes, and Rust can't keep a borrow. The macro sees a reference in a parameter type syntactically (`&T`, `impl Trait + '_`, a lifetime parameter), and makes such composables **non-restartable** (`&'static T` is kept like any value, and `&mut T` is rejected), Compose's `@NonRestartableComposable`. They don't skip and have no restart scope; they run inline whenever their caller runs. This is the main expressiveness difference from Kotlin, where any object reference can be kept and compared by identity. In Rust, data a row should skip on is passed as `Rc<Todo>` or as state handles. See the [todo sketch](../sketches/todo.rs).

## Lambdas: memoized on their captures

Strong skipping only works if lambdas passed as arguments keep their identity across passes. Compose wraps each lambda in `remember(captures) { lambda }`, and the macro does the same, syntactically:

1. **Which closures.** Only `move` closures, and closures that capture nothing, are memoized: only they can be `'static`, and only `'static` closures are stored, as handlers or as content kept for restarts. A borrowing closure, such as the one given to `key(id, || …)` or `state.with(|v| …)`, runs at once and is left untouched. The call macros add `move` to the closures they pass as composable content, so content is always memoizable.
2. For each memoized closure, the macro computes its captures: the free identifiers in the closure that resolve to locals or parameters of the enclosing function. It tracks scopes from `let`, parameters, `for`, `match` arms and closure parameters, in order.
3. It rewrites `move |x| body` into
   ```rust
   compose::remember_lambda((compose::arg_key(&cap1), compose::arg_key(&cap2)), move |x| body)
   ```
   This returns a `Callback` (an `Rc<dyn Fn(X)>`) stored in a slot. While every capture is `same` as last pass, it returns the stored `Rc`, so the callee sees the identical callback and can skip; otherwise it stores the new closure.
4. `arg_key` uses *autoref specialisation*, which works because macro output is type-checked at the use site. A capture whose type implements `ComposeArg` contributes its value. A capture that doesn't makes the lambda count as always changed, instead of being a compile error.
5. **Soundness of the walk.** A real capture must never be missed, or the lambda would wrongly keep an old closure. The walk sees every identifier in the closure's tokens, including those passed to macros. It also scans the string literals of macro calls for inline format arguments (`"{count}"` captures `count`). Declarative macros are hygienic for local variables, so they can't reach a local that isn't in their tokens. Only an unhygienic proc macro could invent such a reference. For that residual case, a closure containing a call to a macro outside a small allow-list (`format!`, `println!`, `vec!`, the composables' own call macros, …) counts as always changed. Over-counting captures is harmless: the lambda is just recreated more often than necessary.

Handlers stored in UI nodes (`on_click`) must be `'static`, so closures capture `Copy` state handles or clones (`move`). `Copy` handles make that the natural case; see [05](05-state.md).

## Groups for control flow

Calls of the same composable inside one group match **in order**. That's Compose's positional memoization, without a group per call. Control flow needs region groups, so state doesn't jump between branches:

| Construct | Group | Key |
|---|---|---|
| each composable body | restart group (or replace group if non-restartable) | declaration path |
| each `if`/`else` branch, `match` arm, `if let` | replace group | structural index in the body (`KEY ^ n`) |
| each loop (`for`, `while`, `while let`, `loop`), as a whole | replace group | structural index |
| `key(id, \|\| …)` / `key!(id, { … })` | movable group | the key's hash, from `ComposeArg + Hash` |
| `remember`, `remember_lambda` | slots in the current group | position |

The macro can't know which calls are composable, so it **adds a region group to a branch only if the branch contains a likely composable call**. A likely composable call is one of:
- a call whose final path segment is PascalCase: `Text(…)`, `material3::Button(…)`;
- a PascalCase macro call: `Text!(…)`;
- `remember`, `remember_lambda`, `key`, `SideEffect`, `DisposableEffect`;
- an invocation of a parameter typed as a composable lambda.

A region group is a guard too: `let __compose_region = region(key ^ n);` (a literal) at the start of the branch, or around the loop. Outside a composition pass it does nothing, because the guess can place one inside a handler closure (`move || if x { Message::Text(s) } …`) that runs later, outside any pass. In a loop with a `continue`, each statement containing a likely composable call gets its own group (below).

Missing a region group is safe: groups carry their callee's key and slots carry their type, so a mismatch removes and recreates state rather than misreading it (the runtime never trusts keys for memory safety; see [04](04-runtime.md)). Branches with no likely composable call get no group, which keeps the cost proportional to UI code.

Loops with a `continue` get one group per call site: a `continue` can skip different calls in different iterations, and in-order matching would then pair a call with another call's group. Keyed items use `key`, and `key` is outermost in the loop body.

## Composable lambdas (content)

A parameter typed `impl Fn() + 'static` whose name or attribute marks it as content is rewritten by the callee's macro to a `Callback<()>`. That makes it comparable by identity and storable in the kept tuple. A content lambda has no restart scope of its own: state read inside it registers the scope of the composable that invokes it. That composable restarts with its kept content callback, which re-runs the lambda, so restarts stay correct with one fewer kind of group. Compose gives content lambdas their own scope, which recomposes less; the cost here is re-running the invoking composable's own body too, usually a layout composable doing almost nothing.

## Named, default and trailing arguments: the call macro

Rust has no named or default arguments, and Compose's API depends on them. `Button(onClick, modifier = Modifier, enabled = true, …, content)` is the norm. So `#[composable]` also generates a declarative macro with the composable's name:

```rust
#[composable]
pub fn Button(
    on_click: impl Fn() + 'static,
    #[default(Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    content: impl Fn() + 'static,
) { … }

// generated: macro_rules! Button, re-exported next to the function (macros and functions
// live in different namespaces, so `use compose::material3::*` brings in both).

Button!(on_click = move || count.update(|c| c + 1), enabled = count.get() > 0, {
    Text!("Increment");
});
```

Rules of the call macro:

- **Positional, then named.** Arguments bind positionally from the first parameter, then by `name = value`, as in Kotlin.
- **Defaults** come from `#[default(expr)]`. A missing parameter without a default is a compile error naming it.
- **Trailing lambda.** A final bare block `{ … }` becomes the last parameter, as a closure `move || { … }`, or `move |_scope| { … }` when the content is scoped (`Column`, `Row`, `Box`). A final explicit closure (`|row| { … }`) is passed as is, with `move` added.
- **Formatting.** A composable whose first parameter is marked `#[format]` (`Text`) accepts format arguments: `Text!("Count: {}", n)`.
- **Plain calls still work.** The function itself stays callable with every argument written out, which is what rust-analyzer completes best.

The generated `macro_rules!` binds names with one arm per parameter, so expansion is linear in the number of arguments written.

**Considered alternatives.**
- **A builder per composable**, as in `Button(on_click).enabled(false).content(|| …)`. It's more IDE-friendly, but it emits on a terminal call, which makes the positional identity of a call depend on where `.content()` or `.show()` is called.
- **An argument struct with `..Default::default()`.** It's verbose at every call site.
- **A single `view!`/`rsx!` DSL for whole bodies.** That moves away from "composables are functions".

## What the macro rejects

| Construct | Why | Diagnostic |
|---|---|---|
| `async fn` composables | composition is synchronous | use `LaunchedEffect` |
| `&mut` parameters | a composable can't hold a mutable borrow across passes | pass a state handle |
| `impl Trait` parameters other than content lambdas | can't be kept | use a concrete type or `Rc<dyn Trait>` |
| composables returning a value | can't restart on their own | allowed, but non-restartable (as in Compose) |

## Cost summary

Measured on a crate of 1,000 composables (each with three parameters, a `remember`, a branch and a loop; debug build, Apple M-series): a one-line edit rebuilds in 0.77 s with `#[composable]`, against 0.17 s for the same functions without it, and a clean build of the crate takes 2.3 s against 0.7 s. About 0.6 ms per composable per edit: re-running the macro on every composable of the crate (proc macros aren't cached between builds) and checking the larger expanded code. An app crate of 100 composables pays about 60 ms per edit, which is why app code stays in small crates ([08](08-fast-builds.md)).

Per composable definition, the macro generates one group, one kept tuple, one comparison per parameter, one `fn` restart pointer and one call macro. Per closure argument, it adds one `remember_lambda` slot. Per branch containing composable calls, one region group. Nothing is generic beyond the composable's own parameters, and the runtime functions take `&dyn Any` or plain words.
