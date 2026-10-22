/// Greets someone.
/**
 # Call

 ```text
 Greeting!(name, count)
 ```

 Arguments go in this order, or by name (`name = value`); those shown with a value are optional.*/
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn Greeting(name: String, count: i32) {
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<(String, i32)>(__kept);
        Greeting(
            ::compose::__private::keep(&__kept.0),
            ::compose::__private::keep(&__kept.1),
        )
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        12070982927730596820u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    let mut __compose_kept = __composer.take_kept();
    let __compose_changed = match __compose_kept.get::<(String, i32)>() {
        ::core::option::Option::Some(__kept) => {
            !::compose::__private::ComposeArg::same(&__kept.0, &name)
                || !::compose::__private::ComposeArg::same(&__kept.1, &count)
        }
        ::core::option::Option::None => true,
    };
    if __compose_changed {
        __compose_kept
            .set((
                ::compose::__private::keep(&name),
                ::compose::__private::keep(&count),
            ));
    }
    if !__compose_kept.put(__compose_changed) && __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    {
        Text(format!("Hello {name}: {count}"));
    }
}
#[doc(hidden)]
#[macro_export]
macro_rules! __compose_call_Greeting_460afe76bd277c02_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Greeting[name : value, count : value] plain @
        path($($path)*); $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Greeting[name : value, count : value] plain;
        $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
pub use __compose_call_Greeting_460afe76bd277c02_N as Greeting;
