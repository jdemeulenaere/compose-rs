use compose::desktop::{WindowOptions, run_app};

fn main() {
    run_app(
        WindowOptions { title: "Gallery".into(), size: (420, 760).into(), ..Default::default() },
        gallery::Gallery,
    );
}
