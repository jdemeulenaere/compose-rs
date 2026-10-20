/// Greets someone.
#[allow(non_snake_case)]
pub fn Greeting(name: String, count: i32) {
    let __composer = ::compose::__private::current();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        12070982927730596820u64,
        |__kept: &dyn ::core::any::Any| {
            let __kept = ::compose::__private::kept::<(String, i32)>(__kept);
            Greeting(
                ::compose::__private::keep(&__kept.0),
                ::compose::__private::keep(&__kept.1),
            )
        },
    );
    if !__composer
        .update_kept(
            |__kept: &(String, i32)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &name)
                    || !::compose::__private::ComposeArg::same(&__kept.1, &count)
            },
            || (::compose::__private::keep(&name), ::compose::__private::keep(&count)),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    Text(format!("Hello {name}: {count}"));
}
