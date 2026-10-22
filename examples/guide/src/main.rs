//! The guide's desktop app: `cargo run -p guide`. In the guide's layout this is
//! `examples/hello/src/main.rs`, where `hello` is the crate's own library.

use guide::getting_started as hello;

// ANCHOR: main
// examples/hello/src/main.rs
use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Hello".into(), size: (360, 240).into(), ..Default::default() },
        hello::Hello,
    );
}
// ANCHOR_END: main
