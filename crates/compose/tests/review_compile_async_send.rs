//! Review: valid code that `#[composable]` makes fail to compile.
//!
//! The region pass descends into `async` blocks. A branch there containing a PascalCase call
//! (here an enum variant, `Event::Clicked`) gets a `region` guard, which holds a `!Send`
//! `Composer` and lives across the `.await`: the future is no longer `Send`, so handing it to a
//! multi-threaded executor fails to compile ("future cannot be sent between threads safely").
//! The guard is pointless there anyway: the future is polled outside composition.
//!
//! Expected: compiles (as it does without `#[composable]`). Actual: E0277, `Region` / `Composer`
//! is not `Send`.

#![allow(non_snake_case, dead_code)]

use std::future::Future;

use compose::runtime::*;

#[derive(Debug)]
enum Event {
    Clicked(u32),
}

fn spawn_send(_future: impl Future<Output = ()> + Send + 'static) {}

async fn deliver(_event: Event) {}

#[composable]
fn Clickable(id: u32) {
    spawn_send(async move {
        if id > 0 {
            deliver(Event::Clicked(id)).await;
        }
    });
}

#[test]
fn compiles() {}
