#[composable]
fn Counter() {
    let count = remember(|| mutable_state_of(0));
    Text(format!("Count: {}", count.get()));
}
