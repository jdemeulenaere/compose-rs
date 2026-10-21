use compose::desktop::{WindowOptions, run_app};

fn main() {
    // `TODO_ITEMS=1000 cargo run -p todo` starts with that many items.
    let items: u64 =
        std::env::var("TODO_ITEMS").ok().and_then(|items| items.parse().ok()).unwrap_or(0);
    run_app(
        WindowOptions { title: "Todo".into(), size: (480, 640).into(), ..Default::default() },
        move || todo::TodoAppWith(items),
    );
}
