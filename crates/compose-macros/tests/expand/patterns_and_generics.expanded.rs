#[allow(non_snake_case)]
fn Pair(__compose_argument0: (i32, i32), _unused: bool) {
    let __composer = ::compose::__private::current();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        17074722474517449155u64,
        |__kept: &dyn ::core::any::Any| {
            let (__compose_argument0, _unused) = ::compose::__private::kept::<
                ((i32, i32), bool),
            >(__kept);
            Pair(__compose_argument0, _unused)
        },
    );
    if !__composer
        .update_kept(
            |__kept: &((i32, i32), bool)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &__compose_argument0)
                    || !::compose::__private::ComposeArg::same(&__kept.1, &_unused)
            },
            || (
                ::compose::__private::keep(&__compose_argument0),
                ::compose::__private::keep(&_unused),
            ),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    let (first, second) = __compose_argument0;
    Text(format!("{first}, {second}"));
}
#[allow(non_snake_case)]
fn List<T: ComposeArg + Clone + Display>(items: Rc<Vec<T>>) {
    let __composer = ::compose::__private::current();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        18251905653550721707u64,
        |__kept: &dyn ::core::any::Any| {
            let (items,) = ::compose::__private::kept::<(Rc<Vec<T>>,)>(__kept);
            List::<T>(items)
        },
    );
    if !__composer
        .update_kept(
            |__kept: &(Rc<Vec<T>>,)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &items)
            },
            || (::compose::__private::keep(&items),),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    {
        let __compose_region = ::compose::__private::region(18251905653550721706u64);
        for item in items.iter() {
            Text(item.to_string());
        }
    }
}
