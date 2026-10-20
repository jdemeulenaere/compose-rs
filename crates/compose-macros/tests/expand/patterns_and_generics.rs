#[composable]
fn Pair((first, second): (i32, i32), _unused: bool) {
    Text(format!("{first}, {second}"));
}

#[composable]
fn List<T: ComposeArg + Clone + Display>(items: Rc<Vec<T>>) {
    for item in items.iter() {
        Text(item.to_string());
    }
}
