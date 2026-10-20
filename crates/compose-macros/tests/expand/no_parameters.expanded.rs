#[allow(non_snake_case)]
fn Counter() {
    let __composer = ::compose::__private::current();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        3955738544659413941u64,
        |_: &dyn ::core::any::Any| Counter(),
    );
    if __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    let count = remember(|| mutable_state_of(0));
    Text(format!("Count: {}", count.get()));
}
