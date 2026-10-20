#[derive(Clone, ComposeArg)]
struct Todo {
    id: u64,
    title: Rc<str>,
    done: OwnedState<bool>,
}
impl ::compose::__private::ComposeArg for Todo {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        ::compose::__private::ComposeArg::same(&self.id, &other.id)
            && ::compose::__private::ComposeArg::same(&self.title, &other.title)
            && ::compose::__private::ComposeArg::same(&self.done, &other.done)
    }
}
#[derive(Clone, ComposeArg)]
struct Wrapper<T>(T, u8);
impl<T: ::compose::__private::ComposeArg> ::compose::__private::ComposeArg
for Wrapper<T> {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        ::compose::__private::ComposeArg::same(&self.0, &other.0)
            && ::compose::__private::ComposeArg::same(&self.1, &other.1)
    }
}
#[derive(Clone, Copy, PartialEq, ComposeArg)]
enum Filter {
    All,
    Active,
    Done,
}
impl ::compose::__private::ComposeArg for Filter {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::All, Self::All) => true,
            (Self::Active, Self::Active) => true,
            (Self::Done, Self::Done) => true,
            _ => false,
        }
    }
}
#[derive(Clone, ComposeArg)]
enum Shape {
    Circle { radius: f32 },
    Rectangle(f32, f32),
    Empty,
}
impl ::compose::__private::ComposeArg for Shape {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Circle { radius: __self0 }, Self::Circle { radius: __other0 }) => {
                ::compose::__private::ComposeArg::same(__self0, __other0)
            }
            (Self::Rectangle(__self0, __self1), Self::Rectangle(__other0, __other1)) => {
                ::compose::__private::ComposeArg::same(__self0, __other0)
                    && ::compose::__private::ComposeArg::same(__self1, __other1)
            }
            (Self::Empty, Self::Empty) => true,
            _ => false,
        }
    }
}
