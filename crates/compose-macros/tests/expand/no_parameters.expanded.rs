#[allow(non_snake_case, clippy::too_many_arguments)]
fn Counter() {
    let __composer = ::compose::__private::current();
    let __compose_restart = |_: &dyn ::core::any::Any| Counter();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        3955738544659413941u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    if __composer.skipping() {
        __composer.skip_to_group_end();
        return;
    }
    {
        let count = remember(|| mutable_state_of(0));
        Text(format!("Count: {}", count.get()));
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Counter_abf9ac1093febf76_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Counter[] plain @ path($($path)*); $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Counter[] plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Counter_abf9ac1093febf76_N as Counter;
