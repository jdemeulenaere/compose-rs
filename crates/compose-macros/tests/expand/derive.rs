#[derive(Clone, ComposeArg)]
struct Todo {
    id: u64,
    title: Rc<str>,
    done: OwnedState<bool>,
}

#[derive(Clone, ComposeArg)]
struct Wrapper<T>(T, u8);

#[derive(Clone, Copy, PartialEq, ComposeArg)]
enum Filter {
    All,
    Active,
    Done,
}

#[derive(Clone, ComposeArg)]
enum Shape {
    Circle { radius: f32 },
    Rectangle(f32, f32),
    Empty,
}
