#[allow(non_snake_case)]
fn Filtered(ids: Rc<Vec<u32>>, on_change: Rc<dyn Fn(&str)>) {
    #![allow(unused_variables)]
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<
            (Rc<Vec<u32>>, Rc<dyn Fn(&str)>),
        >(__kept);
        Filtered(
            ::compose::__private::keep(&__kept.0),
            ::compose::__private::keep(&__kept.1),
        )
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        9868098601948503028u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    if !__composer
        .update_kept(
            |__kept: &(Rc<Vec<u32>>, Rc<dyn Fn(&str)>)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &ids)
                    || !::compose::__private::ComposeArg::same(&__kept.1, &on_change)
            },
            || (::compose::__private::keep(&ids), ::compose::__private::keep(&on_change)),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    {
        {
            let __compose_region = ::compose::__private::region(9868098601948503030u64);
            for &id in ids.iter() {
                if id == 0 {
                    continue;
                }
                key(
                    id,
                    || {
                        let __compose_region = ::compose::__private::region(
                            9868098601948503029u64,
                        );
                        Row(id)
                    },
                );
            }
        }
        let Some(first) = ids.first() else {
            let __compose_region = ::compose::__private::region(9868098601948503031u64);
            {
                Row(0);
                return;
            }
        };
        let task = async move {
            if *first > 0 {
                Event::Clicked(*first).send().await;
            }
        };
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Filtered_5673cd8923f60262 {
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Filtered[ids : value, on_change : value] plain;
        $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Filtered_5673cd8923f60262 as Filtered;
#[allow(non_snake_case)]
fn Conditional(
    #[cfg(feature = "extra")]
    extra: u32,
    mut __compose_argument1: (u32, u32),
) {
    let __compose_marker = || ();
    let __compose_group = ::compose::__private::ReplaceGroup::start(
        18363063588487543881u64,
        ::compose::__private::name_of(&__compose_marker),
    );
    {
        let (ref mut count, step) = __compose_argument1;
        *count += step;
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Conditional_7f76342f2708a77a {
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Conditional[extra : value, __compose_argument1 :
        value] plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Conditional_7f76342f2708a77a as Conditional;
