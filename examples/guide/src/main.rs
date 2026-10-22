// ANCHOR: main
use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Hello".into(), size: (360, 240).into(), ..Default::default() },
        guide::getting_started::Hello,
    );
}
// ANCHOR_END: main
