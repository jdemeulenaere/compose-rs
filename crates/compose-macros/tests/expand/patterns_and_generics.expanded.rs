#[allow(non_snake_case, clippy::too_many_arguments)]
fn Pair(__compose_argument0: (i32, i32), _unused: bool) {
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<((i32, i32), bool)>(__kept);
        Pair(
            ::compose::__private::keep(&__kept.0),
            ::compose::__private::keep(&__kept.1),
        )
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        17074722474517449155u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    let mut __compose_kept = __composer.take_kept();
    let __compose_changed = match __compose_kept.get::<((i32, i32), bool)>() {
        ::core::option::Option::Some(__kept) => {
            !::compose::__private::ComposeArg::same(&__kept.0, &__compose_argument0)
                || !::compose::__private::ComposeArg::same(&__kept.1, &_unused)
        }
        ::core::option::Option::None => true,
    };
    if __compose_changed {
        __compose_kept
            .set((
                ::compose::__private::keep(&__compose_argument0),
                ::compose::__private::keep(&_unused),
            ));
    }
    if !__compose_kept.put(__compose_changed) && __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    {
        let (first, second) = __compose_argument0;
        Text(format!("{first}, {second}"));
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Pair_c102bf861f04c7d2_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Pair[__compose_argument0 : value, _unused : value]
        plain @ path($($path)*); $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Pair[__compose_argument0 : value, _unused : value]
        plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Pair_c102bf861f04c7d2_N as Pair;
#[allow(non_snake_case, clippy::too_many_arguments)]
fn List<T: ComposeArg + Clone + Display + 'static>(items: Rc<Vec<T>>) {
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<(Rc<Vec<T>>,)>(__kept);
        List::<T>(::compose::__private::keep(&__kept.0))
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        18251905653550721707u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    let mut __compose_kept = __composer.take_kept();
    let __compose_changed = match __compose_kept.get::<(Rc<Vec<T>>,)>() {
        ::core::option::Option::Some(__kept) => {
            !::compose::__private::ComposeArg::same(&__kept.0, &items)
        }
        ::core::option::Option::None => true,
    };
    if __compose_changed {
        __compose_kept.set((::compose::__private::keep(&items),));
    }
    if !__compose_kept.put(__compose_changed) && __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    {
        {
            let __compose_region = ::compose::__private::region(18251905653550721706u64);
            for item in items.iter() {
                Text(item.to_string());
            }
        }
    }
}
#[doc(hidden)]
macro_rules! __compose_call_List_c06f50c9b9d68b8a_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { List[items : value] plain @ path($($path)*);
        $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { List[items : value] plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_List_c06f50c9b9d68b8a_N as List;
