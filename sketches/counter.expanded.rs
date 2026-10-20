// Sketch, not compiled: roughly what `#[composable]` and the call macros turn
// sketches/counter.rs into (release build). Names under `__private` are the
// runtime functions generated code calls; applications never call them.

use compose::__private::{arg_key, current, key_of, remember_lambda};

#[allow(non_snake_case)]
fn Counter() {
    // Key: a const hash of the declaration path. Editing the body never changes it.
    const KEY: u64 = key_of(concat!(module_path!(), "::Counter"));
    let c = current(); // thread-local composer of the pass in progress
    c.start_restart_group(KEY);

    // No parameters, so nothing to keep: the body skips whenever the scope isn't invalid.
    if c.skipping() {
        c.skip_to_group_end();
    } else {
        // `remember` is plain library code: a slot in this group.
        let count = compose::runtime::remember(|| compose::runtime::mutable_state_of(0));

        // Column!(modifier = …, vertical_arrangement = …, { … }) → a positional call with the
        // defaults filled in; the trailing block became a lambda, memoized on its captures.
        compose::foundation::layout::Column(
            Modifier.padding(16.dp()),                              // modifier
            compose::foundation::layout::Arrangement::spaced_by(8.dp()), // vertical_arrangement
            compose::ui::Alignment::START,                          // horizontal_alignment: default
            remember_lambda((arg_key(&count),), move |_column: ColumnScope| {
                compose::material3::Button(
                    // on_click: memoized on `count`; same handle → same Callback → Button can skip
                    remember_lambda((arg_key(&count),), move || count.update(|c| c + 1)),
                    Modifier,                                       // modifier: default
                    true,                                           // enabled: default
                    // content: reads `count` when invoked, inside Button's group, so the read
                    // registers Button's restart scope; a click recomposes Button alone.
                    remember_lambda((arg_key(&count),), move || {
                        compose::material3::Text(
                            compose::__private::format_shared!("Count: {}", count.get()),
                            Modifier,
                            /* color, font_size, …: defaults */
                        );
                    }),
                );
                compose::material3::Button(
                    remember_lambda((arg_key(&count),), move || count.set(0)),
                    Modifier,
                    count.get() > 0,                                // enabled, named in the source
                    remember_lambda((), move || {
                        compose::material3::Text(compose::ui::SharedString::from_static("Reset"), Modifier);
                    }),
                );
            }),
        );
    }

    // A non-capturing closure coerces to `fn(&dyn Any)`: registering it allocates nothing.
    c.end_restart_group(|_kept: &dyn std::any::Any| Counter());
}

// What a composable WITH parameters adds, using Button's own expansion as the example:
//
//   #[composable]
//   pub fn Button(on_click: impl Fn() + 'static,
//                 #[default(Modifier)] modifier: Modifier,
//                 #[default(true)] enabled: bool,
//                 content: impl Fn() + 'static) { … }
//
// becomes, inside the restart group:
//
//   let on_click: Callback<()> = Callback::from(on_click);   // identity if already a Callback
//   let content: Callback<()> = Callback::from(content);
//   let changed = c.update_kept(
//       (on_click.clone(), modifier.clone(), enabled, content.clone()),
//       |kept, new| !kept.0.same(&new.0) || !kept.1.same(&new.1)
//                   || !kept.2.same(&new.2) || !kept.3.same(&new.3),
//   );
//   if !changed && c.skipping() { c.skip_to_group_end(); } else { /* body */ }
//   c.end_restart_group(|kept| {
//       let (on_click, modifier, enabled, content) = kept
//           .downcast_ref::<(Callback<()>, Modifier, bool, Callback<()>)>().unwrap().clone();
//       Button(on_click, modifier, enabled, content)
//   });
