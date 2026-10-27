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
    // only when an argument differs. `update_kept`'s steps, inline: no closures (D200).
    let mut __compose_kept = __composer.take_kept();
    let __compose_changed = match __compose_kept.get::<(SharedString, i32)>() {
        Some(kept) => !ComposeArg::same(&kept.0, &name) || !ComposeArg::same(&kept.1, &count),
        None => true,
    };
    if __compose_changed {
        __compose_kept.set((name.clone(), count.clone()));
    }
    if !__compose_kept.put(__compose_changed) && __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    Text(format_shared!("Hello {name}: {count}"));                 // the body, transformed
}
```

The pieces:

- **Group key**: a hash of the declaration, computed by the macro and emitted as a literal: the crate name, the source file (relative to the package, or to `OUT_DIR` for generated code, or just the file's name for code from another crate's macro) and the function's name; region keys are literals too. It doesn't depend on line numbers or formatting, so keys and test dumps stay stable across unrelated edits, and it is the same on every machine. A literal rather than a `const` item: each `const` costs the compiler a body to check and evaluate, about a third of a millisecond per composable.
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
| `ImmutableList<T>` (an `Rc<[T]>`) | `ptr_eq`, or both empty | a list for data models, as kotlinx's `ImmutableList`: `From<Vec<T>>`, `FromIterator`, derefs to a slice (`list[i]`, `list.iter()`, `for item in &list`); `PartialEq` compares contents. A list rebuilt with equal contents is a change: keep it in state or `remember` it |
| `Vec<T>`, `HashMap`, `String`-heavy structs | **not implemented** | deep comparisons are what strong skipping avoids; hold a list as an `ImmutableList`, wrap the rest in `Rc` |
| user structs and enums | `#[derive(ComposeArg)]`: field-wise `same`, so `Rc` fields by identity and small fields by value | the Rust analogue of a stable data class; a field marked `#[compose_arg(eq)]` is compared with `PartialEq` instead (`#[compose_arg(eq)] tags: Vec<String>`, a collection the model knows to be short) |

A composable whose parameter type doesn't implement `ComposeArg` fails to compile, with a note naming the parameter and suggesting `Rc<…>` or `#[composable(non_restartable)]`.

### Borrowed parameters

A restartable composable must keep its arguments across passes, and Rust can't keep a borrow. The macro sees a reference in a parameter type syntactically (`&T`, `impl Trait + '_`, a lifetime parameter), and makes such composables **non-restartable** (`&'static T` is kept like any value, and `&mut T` is rejected), Compose's `@NonRestartableComposable`. They don't skip and have no restart scope; they run inline whenever their caller runs. This is the main expressiveness difference from Kotlin, where any object reference can be kept and compared by identity. In Rust, data a row should skip on is passed as `Rc<Todo>` or as state handles. See the [todo sketch](../sketches/todo.rs).

## Lambdas: memoized on their captures

Strong skipping only works if lambdas passed as arguments keep their identity across passes. Compose wraps each lambda in `remember(captures) { lambda }`, and the macro does the same, syntactically:

1. **Which closures.** The macro sees syntax, not types, so it rewrites only the closures it knows become callbacks. In a composable's call macro (`Button!(on_click = move || …, { … })`), every closure and block argument is *marked* with its captures, by a first statement `::compose::__private::marked!(count);` in its body, and the call macro, which knows its parameters, memoizes those bound to callback parameters and passes the others as written. The marker expands to nothing, and a marked block is still a block, a marked closure still a closure, so a user's own PascalCase macro receiving one (as `$body:block`, `$f:expr` or `|$x:ident| $e:expr`) still works. In plain code, a closure wrapped as `callback(…)` or `Callback::new(…)` is memoized. A closure in a modifier chain's callback modifier (`clickable`, `toggleable`, …) is memoized when the chain starts at `Modifier`, the `modifier` parameter, or a local bound to a modifier (`let base = Modifier.padding(4.dp());`, a branch whose arms are modifier chains, or `let base: Modifier = …`), so a modifier built in steps stays `same`. Every other closure keeps its type: one passed to `map`, `state.update(|v| …)`, `key(id, || …)`, a generic `F: Fn()` parameter, or a PascalCase tuple struct such as `AssertUnwindSafe(|| …)`. Nothing inside `remember`'s calculation is memoized: it runs once, while `remember` holds its slot. Memoized closures get `move` added: a callback is `'static`.
2. For each memoized closure, the macro computes its captures: the free identifiers in the closure that resolve to locals or parameters of the enclosing function. It tracks scopes from `let`, parameters, `for`, `match` arms, `if let` and `while let` (including let chains) and closure parameters, in order, and shadowing inside the closure. In irrefutable patterns (`let`, parameters, `for`, closure parameters) every identifier binds, PascalCase ones included; in refutable ones (`match` arms, `if let`, `let … else`) a PascalCase identifier may be a constant, a unit variant or a binding, so a lambda naming it counts as always changed (except `None`).
3. It rewrites the closure into a probe of the lambda remembered at that position, one expression:
   ```rust
   resolve_lazy_1(lambda_probe(site, ((&&&ArgProbe(&cap1)).arg_key(), (&&&ArgProbe(&cap2)).arg_key())), || {
       let cap1 = cap1.clone(); let cap2 = cap2.clone();
       move |x| body
   })
   ```
   The lambda is built only on a miss, from clones of its captures: the captures stay the caller's, so a `String` or `Rc<Todo>` parameter can be named by a content block, a click handler and the code after them (`Section!({ Label!("Hello {name}") }); Label!("Goodbye {name}")`), and a lambda nested in another closure keeps that closure `Fn`. Captures must be `Clone` (an `Rc` around data that isn't); those named like constants or variants (`LIMIT`, `Home`) are used as written, and keywords and primitive type names (`as f32`) are never captures. `resolve_lazy_N` (for a closure of `N` parameters, up to eight) spells the `Fn` bound out, so an untyped closure parameter (`|text| …`) takes its type from the callback parameter, as in Kotlin's `onValueChange = { text -> … }`.
   The probe compares the captures with the ones kept last pass (borrowing them, so a steady-state pass clones nothing) and returns the remembered `Callback` (an `Rc<dyn Fn(X)>`) when every capture is `same`, dropping the new closure unused; otherwise it stores the new closure with clones of its captures. The callee sees the identical callback and can skip. The keys are a tuple, nested in tuples of twelve when there are more captures.
4. `arg_key` uses *autoref specialisation*, which works because macro output is type-checked at the use site. A capture whose type implements `ComposeArg + Clone` contributes a key. A capture that doesn't makes the lambda count as always changed, instead of being a compile error, unless it is zero-sized (a function item, a closure capturing nothing): every value of such a type is the same. Zero-sized captures are told apart by their `TypeId` (every closure of a function has the same type name), so two capture-less closures reaching one site, from two branches through a generic helper or a user's macro, aren't confused; captures are `'static`, as the callbacks holding them are.
5. **The probe is a slot, so it needs groups like `remember`.** Each probe also carries a site id, a hash of the lambda's code: a slot kept by a different lambda (which a region group missed by the syntactic guess could hand it) is a miss, so a missed group can cost state, but never hands a call another lambda's function. It takes the next slot of the current group, so `lambda_probe` counts as a likely composable call: a branch, loop or closure containing a memoized lambda gets its region group (and so do the right operands of `&&` and `||`, and `match` guards). Without one, two `callback(…)`s in the two branches of an `if` would share a slot (and the else branch get the then branch's closure), and an optional or looped callback would shift the slots after it, losing their state.
6. **Soundness of the walk.** A real capture must never be missed, or the lambda would wrongly keep an old closure. The walk sees every identifier in the closure's tokens, including those passed to macros. It also scans string literals in macro calls for format arguments: inline ones (`"{count}"` captures `count`) and width and precision arguments (`"{:>width$}"`, `"{:.prec$}"`, `"{:0width$}"`); positional indices (`"{0}"`) and what isn't an identifier (`"{m²}"`) aren't names. Declarative macros are hygienic for local variables, so they can't reach a local that isn't in their tokens. Only an unhygienic proc macro could invent such a reference. For that residual case, a closure containing a call to a macro outside a small allow-list (the standard macros, `log`/`tracing` macros and the composables' own call macros), at any depth in macro tokens, counts as always changed. A `macro_rules!` defined in the function body is never trusted, whatever its name: hygiene lets it name the locals in scope where it is defined. A statement macro outside the list may also bind a local the walk can't see (`bind!(current = value.get());`): a lambda naming a new identifier given to such a macro counts as always changed (a local given to it keeps its key, which names whatever binding is in scope where the lambda is, the macro's if it shadowed it); for a PascalCase macro, which may be the user's (`Bind!(current = …)`, `Scoped!(doubled, n * 2, { … })`), so does a lambda naming a new name in its arguments: before a lone `=`, or a whole argument that is a plain identifier or a tuple of them among the leading arguments (a name after a value, `Clickable!("save", save)`, is a function given positionally). In let chains, a `let`'s bindings are in scope in the operands after it. Format strings are read with their escapes resolved. A name found in a format string resolves where the string was written. Over-counting captures is harmless: the lambda is just recreated more often than necessary.

Handlers stored in UI nodes (`on_click`) must be `'static`, so closures capture `Copy` state handles or clones (`move`). `Copy` handles make that the natural case; see [05](05-state.md).

### Captures a lambda would move

A memoized lambda is `Fn`, runs again, and is built from clones of its captures: it can't give a capture away. Kotlin passes references, so naming a capture where Rust would move it is everyday Compose code (`Column { Text(name) }`, `onClick = { scope.launch { drawer.open() } }`). The macro passes a clone there instead (`::compose::__private::reuse(&name)`), by three syntactic rules:

- **A capture given by itself as an argument**, in a memoized lambda: to a function (`Show(name)`, `remember_…(list)`, and in `remember`'s calculation: `remember(|| mutable_state_of(initial))`), a method (`list.push(name)`, `Modifier.focus_requester(requester)`) or a call macro (`Text!(name)`, `state = list`). For a call macro the clone shadows the name in a block around the call, so a user's own PascalCase macro still receives a name. A call macro's arguments after a leading string literal are format arguments, borrowed, and left alone. Anything else is as written: `&name`, `name.field`, `name.method()`, a struct literal's field, a `let`'s value.
- **A `move` closure or an `async move` block in a memoized lambda** gets clones of the lambda's captures it names: `{ let drawer = reuse(&drawer); async move { drawer.open().await } }`. This covers a click handler launching a task and a lazy list's item closures (`scope.items_keyed(shown.len(), move |i| key(shown[i]), move |i| Row(shown[i]))`, two closures naming one `Rc`). Code inside an `async move` block owns what it names and is left alone.
- **A local given by itself to a call macro's value parameter and named by another argument of the call** is cloned for that parameter: a lambda capturing it (`ModalNavigationDrawer!(drawer_state = drawer, { … drawer … })`), which borrows it to compare its captures, or an expression borrowing it (`Chip!(label, selected = label == current)`). The value can't move it, whatever the parameters' order. `call!` does this one: only it knows which parameters take values.

Only the lambda's captures are cloned: they are `Clone` (D115) and can't be moved out of it anyway. What a lambda makes for itself (its locals and parameters) moves as written, even through an `FnOnce` closure in it (`Some(n).map(|n| { let run = move || send(tx, n); … })`), so a value that isn't `Clone` made in a handler still moves into its task. Captures are `Clone` already (D115): a `Copy` handle costs nothing to clone and an `Rc` a count. The rules don't apply at a composable's top level, where a move is fine until the next use (`LaunchedEffect(key, async move { … })` moves what it names: put it after the calls that use the name, or clone), nor to closures there (`SideEffect(move || …)` moves). A PascalCase statement macro given a capture by itself is wrapped in the block with its clone, so names a user's macro binds there aren't visible after it: give such a macro `name.clone()`.

## Groups for control flow

Calls of the same composable inside one group match **in order**. That's Compose's positional memoization, without a group per call. Control flow needs region groups, so state doesn't jump between branches:

| Construct | Group | Key |
|---|---|---|
| each composable body | restart group (or replace group if non-restartable) | declaration path |
| each `if`/`else` branch, `match` arm, `if let` | replace group | one key per conditional (`KEY ^ n`, an `if … else if … else` chain one conditional) with the branch's index as data key: every branch gets one when one makes a likely composable call, an `if` without `else` an empty `else` one, as Compose's compiler. A branch starting where another branch's group is deletes it there, so what it held is removed before what follows (D715), and is inserted without looking further: in a loop without keys the next such group is the next iteration's (D716) |
| each loop (`for`, `while`, `while let`, `loop`), as a whole | replace group | structural index |
| `key(id, \|\| …)` / `key!(id, { … })` | movable group | the key's hash, from `ComposeArg + Hash` |
| `remember`, memoized lambdas | slots in the current group | position |

The macro can't know which calls are composable, so it **adds a region group to a branch only if the branch contains a likely composable call**. A likely composable call is one of:
- a call whose final path segment starts with an uppercase letter: `Text(…)`, `material3::Button(…)`, `H1(…)`;
- a PascalCase macro call: `Text!(…)`;
- `remember`, `key`, `SideEffect`, `DisposableEffect`, and the probe of a memoized lambda;
- a composable returning a value, named in lower case as Compose's: `remember…` (`remember_scroll_state`, `remember_coroutine_scope`), `animate_…`, `…_as_state`, `produce_state`, `update_transition`;
- an invocation of a parameter typed as a composable lambda: a function-typed parameter (`Rc<dyn Fn()>`, `fn()`), one of a generic type bounded by a function trait (`F: Fn()`), or a local closure (`let header = || Row(1);`) that itself makes a likely call.

A user's macro (not PascalCase, not a standard one) whose tokens make a likely composable call is a region too, as it may branch or loop around them (`when!(show, Text!("x"))`). A closure whose body makes a likely composable call is a region of its own, wherever it runs: `flag.then(|| Row(1))` and `list.iter().for_each(|id| Row(*id))` then don't lend their calls' state to a following `Row(2)`.

A region group is a guard too: `let __compose_region = region(key ^ n);` (a literal) at the start of the branch, or around the loop; the `else` block of a `let … else` is a branch too, as are the right operands of `&&` and `||` and `match` guards; a labelled block, which a `break 'label` can leave early, gets one group per statement, as a loop with a `continue` does. Outside a composition pass it does nothing, because the guess can place one inside a handler closure (`move || if x { Message::Text(s) } …`) that runs later, outside any pass. None goes into `async` blocks or closures (futures run outside composition, and a guard held across an `.await` would make them `!Send`) or `const` blocks. In a loop with a `continue`, each statement containing a likely composable call gets its own group (below).

Missing a region group is memory-safe: groups carry their callee's key and slots carry their type, so a mismatch removes and recreates state rather than misreading it (the runtime never trusts keys for memory safety; see [04](04-runtime.md)). It is not always harmless: when the missed call and a later one call the same composable, the later call can take the earlier one's state. Hence the rules above err towards more groups, which cost a little but are always correct. Branches with no likely composable call get no group, which keeps the cost proportional to UI code.

Loops with a `continue` get one group per call site: a `continue` can skip different calls in different iterations, and in-order matching would then pair a call with another call's group. A `break` out of a labelled block inside the loop counts as a `continue`, statements inside plain and labelled blocks get their own groups, and so does a loop containing a macro other than the standard ones (a macro may expand to a `continue` that isn't in its tokens, and a user macro may be named like a composable). Such a macro statement is grouped by a guard around it rather than a block, so bindings it declares stay in scope. Keyed items use `key`, and `key` is outermost in the loop body: a `key(…)` statement (or `let x = key(…)`) never gets a per-statement group, so items still match by key across the loop.

The syntactic scan of parameter types ignores references inside function types: `Rc<dyn Fn(&str)>`, `fn(&str)` and `for<'a> …` are not borrows, so such callbacks keep a composable restartable.

## Composable lambdas (content)

A parameter typed `impl Fn(A, …) -> R + 'static` is rewritten by the callee's macro to `Callback<dyn Fn(A, …) -> R>`, which makes it comparable by identity and storable in the kept tuple, and keeps the composable non-generic (one instantiation, whatever closures callers pass). The body calls it as before (`content()`): a `Callback` derefs to the function. Callback arguments are passed by value (`impl Fn(String)`, as Compose's `onValueChange: (String) -> Unit`); a callback taking a reference (`impl Fn(&str)`) isn't supported, since its higher-ranked type would make the conversion from closures ambiguous: the macro rejects it at the definition, suggesting an owned type. A plain call passes a `Callback`: `Button(callback(move || …), Modifier, true, callback(move || { … }))`.

A restartable composable's parameter typed `impl Into<T>` is rewritten to `T` in the same way, and its call macro converts what it's given (`Into::into`), its `#[default]` included: `BasicTextField!(…, visual_transformation = PasswordVisualTransformation::new())` takes the password's transformation without `.into()`, and the composable stays non-generic. A plain call passes a `T`; an argument already converted (`x.into()`) leaves the conversion's target ambiguous and doesn't compile. A non-restartable composable (or one returning a value) keeps `impl Into<T>` generic (D232).

A content lambda has no restart scope of its own: state read inside it registers the scope of the composable that invokes it. That composable restarts with its kept content callback, which re-runs the lambda, so restarts stay correct with one fewer kind of group. Compose gives content lambdas their own scope, which recomposes less; the cost here is re-running the invoking composable's own body too, usually a layout composable doing almost nothing. A content block making composable calls is a region group, as any such closure is, and the branches and loops inside it get theirs.

### Root content: call a composable function

The closures a host is given (`UiHost::set_content`, `run_app`, `application`, `compose_test`, `Composition::set_content`) are plain Rust: no `#[composable]` attribute sees them, so the closures and blocks inside them get no region groups and no memoization. Branches there (`if shown { Text!(…) }`) then match nodes by position, so a text shown or hidden above another hands its node (and its state) on, and content lambdas are new at each pass, so what they show never skips. Give a host a closure that only calls a `#[composable]` function (`run_app(options, App)`, `compose_test(|| Counter())`, `application(App)`), which gets both. A block macro can't do it for the closure: it can't tell the closure's captured locals from globals, and would keep lambdas built from stale captures.

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

- **Positional, then named.** Arguments bind positionally from the first parameter, then by `name = value`, as in Kotlin. Only the trailing lambda may follow named arguments.
- **Defaults** come from `#[default(expr)]`. A missing parameter without a default is a compile error naming it. Default expressions are resolved where the call macro is used, as macro arguments are (a declarative macro can't name the callee's module), except paths starting with `crate::`, which the macro turns into `$crate::` and so resolve in the defining crate. Library composables write defaults as `crate::…` paths to public (possibly `#[doc(hidden)]`) items, or as absolute paths (`::compose::ui::Modifier`), so callers need nothing in scope. A defaulted callback is memoized on itself, so the callee still skips.
- **Trailing lambda.** When the last parameter is a callback not given by name, a final bare block `{ … }` becomes it, as a closure `move || { … }`, or `move |_scope| { … }` when the content is scoped (`Column`, `Row`, `Box`); a final explicit closure (`|row| { … }`) is passed as is, with `move` added. A final block or closure is the trailing lambda only once every required parameter before the last is given: `BasicTextField!(value, move |text| …)` binds the closure to `on_value_change`, not to the trailing `decoration_box`. A final block or closure declined that way while a required parameter is missing reports that parameter (`Button!({ … })` lacks `on_click`). As Kotlin's trailing lambda follows the parentheses, a block or closure may also follow named arguments, anywhere after them (`Button!(on_click = …, { … }, enabled = false)`); a value there (`Card!(enabled = false, handle_click)`) is an error, more likely a misplaced argument than the content.
- **Formatting.** A composable whose first parameter is marked `#[format]` (`Text`) accepts format arguments: `Text!("Count: {}", n)`, including inline ones (`Text!("Count: {n}")`) and named ones (`Text!("{count} items", count = items.len())`: a named argument the string names is a format argument, as in `format!`). Whatever it is given is converted with `Into` (`Text!(title)` with a `SharedString`).
- **Callback arguments.** A closure given for a callback parameter is memoized (above), and so is a block, as a content lambda (`content = { … }`, as in Kotlin). A callback given by name (`on_click = on_remove`) is cloned rather than moved, so a content lambda passing it on stays `Fn`, and memoized on itself: a `Callback`, a function or a capture-less closure passes the same `Callback` every pass. For an optional callback (`Option<impl Fn()>`), a lambda or `Some(lambda)` is `Some` memoized callback, `None` is `None`, and anything else converts through `IntoOptionalCallback`, as Kotlin passes a lambda for a nullable one: another `Option` (`if editable { Some(move || …) } else { None }`, its closure unmemoized), a callback (`Card!(on_click = on_click, …)` forwarding a required callback into an optional one), a function or a closure. Given by name, any of them is memoized on itself, as a required callback is: the same value passes the same callback, and the callee skips. A composable's call given for a callback (`top_bar = TopAppBar!(…)`) is an error suggesting the braces of a content lambda (`top_bar = { TopAppBar!(…) }`).
- **Called by path.** `ui::Label!(…)` works as `ui::Label(…)` does: inside a composable, the macro passes the path first (`ui::Label!(@path (ui::Label) …)`) and the expansion calls the function by it, since the call macro's expansion calls the bare name, which would otherwise resolve to whatever `Label` is in scope at the call site.
- **Errors** name the problem at its place: an unknown parameter (listing the parameters), an argument given twice, a positional argument after a named one (pointing at the argument), a missing argument, a composable's call given for a callback.
- **Plain calls still work.** The function itself stays callable with every argument written out, which is what rust-analyzer completes best.
- **Documented.** `#[composable]` appends a "Call" section to a documented function's doc comment (an undocumented one stays undocumented, for `missing_docs`) with the call macro's form and the defaults, so rustdoc shows which arguments are optional:
  ```text
  Button!(on_click, modifier = Modifier, enabled = true, …, { content })
  ```
  (one argument per line when it's long; `crate::` left out of the defaults).

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
| `impl Trait` parameters other than content lambdas and `impl Into<T>` | can't be kept | use a concrete type or `Rc<dyn Trait>` |
| composables returning a value | can't restart on their own | allowed, but non-restartable (as in Compose) |
| callback parameters taking references (`impl Fn(&str)`) | a closure converts to `dyn Fn(&T)` for one lifetime only | take an owned type |

## Cost summary

Measured on a crate of 1,000 composables (each with three parameters, a `remember`, a branch and a loop; debug build, Apple M-series): a one-line edit rebuilds in 0.77 s with `#[composable]`, against 0.17 s for the same functions without it, and a clean build of the crate takes 2.3 s against 0.7 s. About 0.6–0.7 ms per composable per edit (a review measured 1.04 s against 0.35 s on the same shape), plus about 0.1 ms per closure making composable calls (closures are regions): re-running the macro on every composable of the crate (proc macros aren't cached between builds) and checking the larger expanded code. Phase C code costs more per edit: a call macro about 0.05 ms, and a lambda about 0.4 ms, of which the memoization probe is about 0.1 ms and the rest the closure itself (its type, its `Callback` conversion), as in plain Rust (measured on 1,000 composables with four call macros and five lambdas each: 2.35 s per edit, against 1.9 s with the same closures unmemoized and 1.3 s without them). Building every lambda lazily from clones of its captures (D115) added about 0.15 ms per lambda (a review measured 500 composables with five lambdas each: 2.96 s per edit before, 3.34 s after). An app crate of 100 composables in that style pays about 250 ms per edit, which is why app code stays in small crates ([08](08-fast-builds.md)). Cloning captures a lambda would move and the generated call-form docs added about 30 µs per composable to the expansion (rustc's `expand_crate` for compose-material3's 50 composables: 51.7 ms before, 53.3 ms after; the gallery's: 31 ms before, 32.5 ms after, its rebuild otherwise unchanged within noise).

Code generation is the other half: at `opt-level = 0` every instantiation of a generic function is a function of its own, which rustc collects, lowers and hands to LLVM, and generated code instantiates the runtime's generic entry points per composable and per call site. So these are thin shims over cores instantiated per type, or not generic at all (D200), as std's `Vec` is over `RawVec`: `remember` and `remember_keyed` match on `remembered::<T>` and call `calc` only on a miss (72 and 100 IR lines per call site, against 260 and 460 before, plus 157 and 115 per remembered type and 217 per keys type); a composable writes `update_kept`'s steps inline, so it instantiates no closures and only its kept tuple's `KeptArguments::get` and `set` (44 and 147 lines, against 330 for `update_kept`); a lambda's miss wraps it and calls `PendingLambda::store_callback`, instantiated per captures' types and signature instead of per lambda (190 lines each before); `CompositionLocalProvider` starts its provider in a function per value type. LLVM IR of each crate's own code generation (`--emit=llvm-ir`, one codegen unit), before and after: compose-material3 241.7k → 206.5k lines (−15%), compose-foundation 165.1k → 157.6k (−5%), the gallery 71.1k → 60.9k (−14%). Clean builds gain less, the type and borrow checks of the expanded bodies weighing as much as code generation: medians of three interleaved runs, material3's library 0.79 → 0.75 s and with its tests 1.90 → 1.80 s, foundation's 0.69 → 0.70 s and 2.74 → 2.64 s, the gallery's 0.36 → 0.35 s and 0.71 → 0.69 s. What remains per lambda is mostly its own: the closure, its `Rc::new`, its drop glue, and the probe per distinct captures' types (in the gallery, `lambda_probe` and `store_callback` are a fifth of the IR).

Per composable definition, the macro generates one group, one kept tuple, one comparison per parameter, one `fn` restart pointer and one call macro. Per memoized lambda, it adds one slot and one comparison per capture. Per branch containing composable calls, one region group. Nothing is generic beyond the composable's own parameters and its kept tuple's `get` and `set`, and the runtime functions take `&dyn Any` or plain words.
