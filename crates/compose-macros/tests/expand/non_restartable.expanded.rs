#[allow(non_snake_case)]
fn Borrowed(title: &str) {
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        11020714633250996767u64,
    );
    Text(title.to_string());
}
#[allow(non_snake_case)]
fn Returns(value: i32) -> i32 {
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        1115807124531480470u64,
    );
    remember(|| value * 2)
}
#[allow(non_snake_case)]
fn Explicit(content: impl Fn()) {
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        18085491760640374923u64,
    );
    content();
}
#[allow(non_snake_case)]
fn StaticReference(title: &'static str) {
    let __composer = ::compose::__private::current();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        8726448151579878150u64,
        |__kept: &dyn ::core::any::Any| {
            let __kept = ::compose::__private::kept::<(&'static str,)>(__kept);
            StaticReference(::compose::__private::keep(&__kept.0))
        },
    );
    if !__composer
        .update_kept(
            |__kept: &(&'static str,)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &title)
            },
            || (::compose::__private::keep(&title),),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    Text(title.to_string());
}
