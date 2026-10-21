use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Counter".into(), size: (320, 200).into(), ..Default::default() },
        counter::Counter,
    );
}
