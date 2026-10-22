#[allow(non_snake_case, clippy::too_many_arguments)]
fn Borrowed(title: &str) {
    let __compose_marker = || ();
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        11020714633250996767u64,
        ::compose::__private::name_of(&__compose_marker),
    );
    {
        Text(title.to_string());
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Borrowed_1efc7a945654dc5e_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Borrowed[title : value] plain @ path($($path)*);
        $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Borrowed[title : value] plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Borrowed_1efc7a945654dc5e_N as Borrowed;
#[allow(non_snake_case, clippy::too_many_arguments)]
fn Returns(value: i32) -> i32 {
    let __compose_marker = || ();
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        1115807124531480470u64,
        ::compose::__private::name_of(&__compose_marker),
    );
    { remember(|| value * 2) }
}
#[doc(hidden)]
macro_rules! __compose_call_Returns_0ad3229f33d2e0b2_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Returns[value : value] plain @ path($($path)*);
        $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Returns[value : value] plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Returns_0ad3229f33d2e0b2_N as Returns;
#[allow(non_snake_case, clippy::too_many_arguments)]
fn Explicit(content: impl Fn()) {
    let __compose_marker = || ();
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        18085491760640374923u64,
        ::compose::__private::name_of(&__compose_marker),
    );
    {
        content();
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Explicit_0e6aa01d864d81a2_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Explicit[content : value] plain @ path($($path)*);
        $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Explicit[content : value] plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Explicit_0e6aa01d864d81a2_N as Explicit;
#[allow(non_snake_case, clippy::too_many_arguments)]
fn StaticReference(title: &'static str) {
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<(&'static str,)>(__kept);
        StaticReference(::compose::__private::keep(&__kept.0))
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        8726448151579878150u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    let mut __compose_kept = __composer.take_kept();
    let __compose_changed = match __compose_kept.get::<(&'static str,)>() {
        ::core::option::Option::Some(__kept) => {
            !::compose::__private::ComposeArg::same(&__kept.0, &title)
        }
        ::core::option::Option::None => true,
    };
    if __compose_changed {
        __compose_kept.set((::compose::__private::keep(&title),));
    }
    if !__compose_kept.put(__compose_changed) && __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    {
        Text(title.to_string());
    }
}
#[doc(hidden)]
macro_rules! __compose_call_StaticReference_083104b1423b9cc6_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { StaticReference[title : value] plain @
        path($($path)*); $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { StaticReference[title : value] plain;
        $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_StaticReference_083104b1423b9cc6_N as StaticReference;
