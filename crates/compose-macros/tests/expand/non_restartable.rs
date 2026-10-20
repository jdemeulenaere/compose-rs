#[composable]
fn Borrowed(title: &str) {
    Text(title.to_string());
}

#[composable]
fn Returns(value: i32) -> i32 {
    remember(|| value * 2)
}

#[composable(non_restartable)]
fn Explicit(content: impl Fn()) {
    content();
}

#[composable]
fn StaticReference(title: &'static str) {
    Text(title.to_string());
}
