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
- **Free functions only.** A composable can't be a method or an associated function: its restart function names it unqualified, and a `self` receiver is a borrow. Nor can it be `const` or `async`. Parameters written as patterns (`(x, y): (i32, i32)`) are renamed and destructured at the start of the body; a `#[cfg]`-ed parameter makes the composable non-restartable, since it may not exist to be kept.
- **Colliding keys never match.** Two composables can share a key: two instantiations of a generic composable, or same-named composables in one file's inline modules. A restart group remembers its composable's identity, the `TypeId` of its restart closure (unique to each composable and each instantiation of a generic one, and the same in every copy of an `#[inline]` composable, unlike the closure's address), and a group with another identity doesn't match: the composable gets a group of its own, never another one's kept arguments or state. So a restartable generic composable's type parameters must be `'static`, which the macro adds as a bound; its kept arguments are `'static` already, so this only shows for a type parameter that no parameter uses (`Phantom::<T>()`), where a generic caller then needs `T: 'static` too, as rustc's error says. A non-restartable composable's replace group is identified by the type name of a marker closure instead (its parameters may borrow, so no `TypeId`): unique per composable and instantiation, compared by address first. A parameter named like its composable (`fn label(label: String)`) is renamed internally, so it can't shadow the composable for the restart function.
- **The body is a block of its own**, so an item it declares (`use other::Title;`) can't shadow the composable for the restart function. `unsafe fn` composables are rejected: the runtime calls them again from safe code.
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

1. **Which closures.** The macro sees syntax, not types, so it rewrites only the closures it knows become callbacks. In a composable's call macro (`Button!(on_click = move || …, { … })`), every closure and block argument is *marked* with its captures (`::compose::__private::marked!((count) move || …)`), and the call macro, which knows its parameters, memoizes those bound to callback parameters and passes the others as written. The marker is itself a macro expanding to the code as written, so a user's own PascalCase macro receiving one still works. In plain code, a closure wrapped as `callback(…)` or `Callback::new(…)` is memoized. Every other closure keeps its type: one passed to `map`, `state.update(|v| …)`, `key(id, || …)`, a generic `F: Fn()` parameter, or a PascalCase tuple struct such as `AssertUnwindSafe(|| …)`. Nothing inside `remember`'s calculation is memoized: it runs once, while `remember` holds its slot. Memoized closures get `move` added: a callback is `'static`.
2. For each memoized closure, the macro computes its captures: the free identifiers in the closure that resolve to locals or parameters of the enclosing function. It tracks scopes from `let`, parameters, `for`, `match` arms, `if let` and `while let` (including let chains) and closure parameters, in order, and shadowing inside the closure. In irrefutable patterns (`let`, parameters, `for`, closure parameters) every identifier binds, PascalCase ones included; in refutable ones (`match` arms, `if let`, `let … else`) a PascalCase identifier is taken for a constant or a unit variant (`None`).
3. It rewrites the closure into a probe of the lambda remembered at that position, one expression:
   ```rust
   lambda_probe(((&&ArgProbe(&cap1)).arg_key(), (&&ArgProbe(&cap2)).arg_key())).resolve(move |x| body)
   ```
   The probe compares the captures with the ones kept last pass (borrowing them, so a steady-state pass clones nothing) and returns the remembered `Callback` (an `Rc<dyn Fn(X)>`) when every capture is `same`, dropping the new closure unused; otherwise it stores the new closure with clones of its captures. The callee sees the identical callback and can skip. The keys are a tuple, nested in tuples of twelve when there are more captures.
4. `arg_key` uses *autoref specialisation*, which works because macro output is type-checked at the use site. A capture whose type implements `ComposeArg + Clone` contributes a key. A capture that doesn't makes the lambda count as always changed, instead of being a compile error, unless it is zero-sized (a function item, a closure capturing nothing): every value of such a type is the same.
5. **The probe is a slot, so it needs groups like `remember`.** It takes the next slot of the current group, so `lambda_probe` counts as a likely composable call: a branch, loop or closure containing a memoized lambda gets its region group. Without one, two `callback(…)`s in the two branches of an `if` would share a slot (and the else branch get the then branch's closure), and an optional or looped callback would shift the slots after it, losing their state.
6. **Soundness of the walk.** A real capture must never be missed, or the lambda would wrongly keep an old closure. The walk sees every identifier in the closure's tokens, including those passed to macros. It also scans string literals in macro calls for format arguments: inline ones (`"{count}"` captures `count`) and width and precision arguments (`"{:>width$}"`, `"{:.prec$}"`); positional indices (`"{0}"`) aren't names. Declarative macros are hygienic for local variables, so they can't reach a local that isn't in their tokens. Only an unhygienic proc macro could invent such a reference. For that residual case, a closure containing a call to a macro outside a small allow-list (the standard macros, `log`/`tracing` macros and the composables' own call macros) counts as always changed. A statement macro outside that list may also bind a local the walk can't see (`bind!(current = value.get());`): a lambda naming any identifier given to such a macro counts as always changed. Over-counting captures is harmless: the lambda is just recreated more often than necessary.

Handlers stored in UI nodes (`on_click`) must be `'static`, so closures capture `Copy` state handles or clones (`move`). `Copy` handles make that the natural case; see [05](05-state.md).

## Groups for control flow

Calls of the same composable inside one group match **in order**. That's Compose's positional memoization, without a group per call. Control flow needs region groups, so state doesn't jump between branches:

| Construct | Group | Key |
|---|---|---|
| each composable body | restart group (or replace group if non-restartable) | declaration path |
| each `if`/`else` branch, `match` arm, `if let` | replace group | structural index in the body (`KEY ^ n`) |
| each loop (`for`, `while`, `while let`, `loop`), as a whole | replace group | structural index |
| `key(id, \|\| …)` / `key!(id, { … })` | movable group | the key's hash, from `ComposeArg + Hash` |
| `remember`, memoized lambdas | slots in the current group | position |

The macro can't know which calls are composable, so it **adds a region group to a branch only if the branch contains a likely composable call**. A likely composable call is one of:
- a call whose final path segment is PascalCase: `Text(…)`, `material3::Button(…)`;
- a PascalCase macro call: `Text!(…)`;
- `remember`, `key`, `SideEffect`, `DisposableEffect`, and the probe of a memoized lambda;
- an invocation of a parameter typed as a composable lambda: a function-typed parameter (`Rc<dyn Fn()>`, `fn()`), one of a generic type bounded by a function trait (`F: Fn()`), or a local closure (`let header = || Row(1);`) that itself makes a likely call.

A closure whose body makes a likely composable call is a region of its own, wherever it runs: `flag.then(|| Row(1))` and `list.iter().for_each(|id| Row(*id))` then don't lend their calls' state to a following `Row(2)`.

A region group is a guard too: `let __compose_region = region(key ^ n);` (a literal) at the start of the branch, or around the loop; the `else` block of a `let … else` is a branch too. Outside a composition pass it does nothing, because the guess can place one inside a handler closure (`move || if x { Message::Text(s) } …`) that runs later, outside any pass. None goes into `async` blocks or closures (futures run outside composition, and a guard held across an `.await` would make them `!Send`) or `const` blocks. In a loop with a `continue`, each statement containing a likely composable call gets its own group (below).

Missing a region group is memory-safe: groups carry their callee's key and slots carry their type, so a mismatch removes and recreates state rather than misreading it (the runtime never trusts keys for memory safety; see [04](04-runtime.md)). It is not always harmless: when the missed call and a later one call the same composable, the later call can take the earlier one's state. Hence the rules above err towards more groups, which cost a little but are always correct. Branches with no likely composable call get no group, which keeps the cost proportional to UI code.

Loops with a `continue` get one group per call site: a `continue` can skip different calls in different iterations, and in-order matching would then pair a call with another call's group. A `break` out of a labelled block inside the loop counts as a `continue`, statements inside plain and labelled blocks get their own groups, and so does a loop containing a macro other than the standard ones (a macro may expand to a `continue` that isn't in its tokens, and a user macro may be named like a composable). Such a macro statement is grouped by a guard around it rather than a block, so bindings it declares stay in scope. Keyed items use `key`, and `key` is outermost in the loop body: a `key(…)` statement (or `let x = key(…)`) never gets a per-statement group, so items still match by key across the loop.

The syntactic scan of parameter types ignores references inside function types: `Rc<dyn Fn(&str)>`, `fn(&str)` and `for<'a> …` are not borrows, so such callbacks keep a composable restartable.

## Composable lambdas (content)

A parameter typed `impl Fn(A, …) -> R + 'static` is rewritten by the callee's macro to `Callback<dyn Fn(A, …) -> R>`, which makes it comparable by identity and storable in the kept tuple, and keeps the composable non-generic (one instantiation, whatever closures callers pass). The body calls it as before (`content()`): a `Callback` derefs to the function. Callback arguments are passed by value (`impl Fn(String)`, as Compose's `onValueChange: (String) -> Unit`); a callback taking a reference (`impl Fn(&str)`) isn't supported, since its higher-ranked type would make the conversion from closures ambiguous: the macro rejects it at the definition, suggesting an owned type. A plain call passes a `Callback`: `Button(callback(move || …), Modifier, true, callback(move || { … }))`.

A content lambda has no restart scope of its own: state read inside it registers the scope of the composable that invokes it. That composable restarts with its kept content callback, which re-runs the lambda, so restarts stay correct with one fewer kind of group. Compose gives content lambdas their own scope, which recomposes less; the cost here is re-running the invoking composable's own body too, usually a layout composable doing almost nothing. A content block making composable calls is a region group, as any such closure is, and the branches and loops inside it get theirs.

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
- **Defaults** come from `#[default(expr)]`. A missing parameter without a default is a compile error naming it. Default expressions are resolved where the call macro is used, as macro arguments are (a declarative macro can't name the callee's module), except paths starting with `crate::`, which the macro turns into `$crate::` and so resolve in the defining crate. Library composables write defaults as `crate::…` paths to public (possibly `#[doc(hidden)]`) items, or as absolute paths (`::compose::ui::Modifier`), so callers need nothing in scope. A defaulted callback is memoized on itself, so the callee still skips.
- **Trailing lambda.** A final bare block `{ … }` becomes the last parameter, as a closure `move || { … }`, or `move |_scope| { … }` when the content is scoped (`Column`, `Row`, `Box`). A final explicit closure (`|row| { … }`) is passed as is, with `move` added.
- **Formatting.** A composable whose first parameter is marked `#[format]` (`Text`) accepts format arguments: `Text!("Count: {}", n)`, including inline ones (`Text!("Count: {n}")`). Whatever it is given is converted with `Into` (`Text!(title)` with a `SharedString`).
- **Callback arguments.** A closure given for a callback parameter is memoized (above), and so is a block, as a content lambda (`content = { … }`, as in Kotlin). A callback given by name (`on_click = on_remove`) is cloned rather than moved, so a content lambda passing it on stays `Fn`, and memoized on itself: a `Callback`, a function or a capture-less closure passes the same `Callback` every pass.
- **Called by path.** `ui::Label!(…)` works as `ui::Label(…)` does: inside a composable, the macro rewrites it to `{ use ui::Label; Label!(…) }`, since the call macro's expansion calls the bare name, which would otherwise resolve to whatever `Label` is in scope at the call site.
- **Errors** name the problem at its place: an unknown parameter (listing the parameters), an argument given twice, a positional argument after a named one, a missing argument.
- **Plain calls still work.** The function itself stays callable with every argument written out, which is what rust-analyzer completes best.

The generated `macro_rules!` has one arm: it forwards its arguments, with a spec of the parameters (names, callback arities, defaults, the format flag), to one shared function-like proc macro, `call!`, which binds them. One shared macro rather than a recursive `macro_rules!` muncher per composable: a macro-expanded `#[macro_export]` macro can't refer to itself by absolute path within its crate. A `pub` composable's call macro is `#[macro_export]`ed under a hidden unique name and re-exported next to the function (`pub use __compose_call_Button_… as Button;`, the hidden name made unique by the declaration's site and a per-expansion serial, so composables a `macro_rules!` generates in several modules don't collide); the others are plain `macro_rules!` re-exported with the function's visibility. So `Name!` is in scope wherever `Name` is imported, and its expansion calls `Name(…)` there.

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
| callback parameters taking references (`impl Fn(&str)`) | a closure converts to `dyn Fn(&T)` for one lifetime only | take an owned type |

## Cost summary

Measured on a crate of 1,000 composables (each with three parameters, a `remember`, a branch and a loop; debug build, Apple M-series): a one-line edit rebuilds in 0.77 s with `#[composable]`, against 0.17 s for the same functions without it, and a clean build of the crate takes 2.3 s against 0.7 s. About 0.6–0.7 ms per composable per edit (a review measured 1.04 s against 0.35 s on the same shape), plus about 0.1 ms per closure making composable calls (closures are regions): re-running the macro on every composable of the crate (proc macros aren't cached between builds) and checking the larger expanded code. Phase C code costs more per edit: a call macro about 0.05 ms, and a lambda about 0.4 ms, of which the memoization probe is about 0.1 ms and the rest the closure itself (its type, its `Callback` conversion), as in plain Rust (measured on 1,000 composables with four call macros and five lambdas each: 2.35 s per edit, against 1.9 s with the same closures unmemoized and 1.3 s without them). An app crate of 100 composables in that style pays about 250 ms per edit, which is why app code stays in small crates ([08](08-fast-builds.md)).

Per composable definition, the macro generates one group, one kept tuple, one comparison per parameter, one `fn` restart pointer and one call macro. Per memoized lambda, it adds one slot and one comparison per capture. Per branch containing composable calls, one region group. Nothing is generic beyond the composable's own parameters, and the runtime functions take `&dyn Any` or plain words.
