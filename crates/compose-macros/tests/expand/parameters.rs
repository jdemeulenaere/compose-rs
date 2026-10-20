/// Greets someone.
#[composable]
pub fn Greeting(name: String, count: i32) {
    Text(format!("Hello {name}: {count}"));
}
