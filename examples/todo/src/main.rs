use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Todo".into(), size: (480, 640).into(), ..Default::default() },
        todo::TodoApp,
    );
}
