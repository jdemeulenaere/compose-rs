/// A button: its call form is appended to these docs.
/**
 # Call

 ```text
 Button!(on_click, modifier = Modifier, enabled = true, { content })
 ```

 Arguments go in this order, or by name (`name = value`); those shown with a value are optional. A final block `{ … }` (or closure) is `content`.*/
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn Button(
    on_click: ::compose::__private::Callback<dyn Fn()>,
    modifier: Modifier,
    enabled: bool,
    content: ::compose::__private::Callback<dyn Fn()>,
) {
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<
            (
                ::compose::__private::Callback<dyn Fn()>,
                Modifier,
                bool,
                ::compose::__private::Callback<dyn Fn()>,
            ),
        >(__kept);
        Button(
            ::compose::__private::keep(&__kept.0),
            ::compose::__private::keep(&__kept.1),
            ::compose::__private::keep(&__kept.2),
            ::compose::__private::keep(&__kept.3),
        )
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        14828256635400177163u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    if !__composer
        .update_kept(
            |
                __kept: &(
                    ::compose::__private::Callback<dyn Fn()>,
                    Modifier,
                    bool,
                    ::compose::__private::Callback<dyn Fn()>,
                )|
            {
                !::compose::__private::ComposeArg::same(&__kept.0, &on_click)
                    || !::compose::__private::ComposeArg::same(&__kept.1, &modifier)
                    || !::compose::__private::ComposeArg::same(&__kept.2, &enabled)
                    || !::compose::__private::ComposeArg::same(&__kept.3, &content)
            },
            || (
                ::compose::__private::keep(&on_click),
                ::compose::__private::keep(&modifier),
                ::compose::__private::keep(&enabled),
                ::compose::__private::keep(&content),
            ),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    {
        Surface(modifier, enabled, content);
    }
}
#[doc(hidden)]
#[macro_export]
macro_rules! __compose_call_Button_ceda1bb746811f5a_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Button[on_click : callback 0, modifier : value = {
        Modifier }, enabled : value = { true }, content : callback 0] plain @
        path($($path)*); $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Button[on_click : callback 0, modifier : value = {
        Modifier }, enabled : value = { true }, content : callback 0] plain;
        $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
pub use __compose_call_Button_ceda1bb746811f5a_N as Button;
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
        Button!(
            on_click = move | | { ::compose::__private::marked!(count); count.update(| c
            | c + 1) }, enabled = count.get() > 0, {
            ::compose::__private::marked!(count); let __compose_region =
            ::compose::__private::region(3955738544659413940u64); Text!("Count: {}",
            count.get()); }
        );
        Toolbar({
            #[allow(unused_imports)]
            use ::compose::__private::{
                KeyedProbe as _, StaticProbe as _, UnkeyedProbe as _,
            };
            ::compose::__private::resolve_lazy_0(
                ::compose::__private::lambda_probe(
                    1804561070696882738u64,
                    ((&&&::compose::__private::ArgProbe(&count)).arg_key(),),
                ),
                || {
                    #[allow(unused_variables)]
                    let count = ::core::clone::Clone::clone(&count);
                    move || count.set(0)
                },
            )
        });
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
#[allow(non_snake_case, clippy::too_many_arguments)]
fn Menu(drawer: DrawerState, title: String, shown: Rc<Vec<u32>>) {
    let __composer = ::compose::__private::current();
    let __compose_restart = |__kept: &dyn ::core::any::Any| {
        let __kept = ::compose::__private::kept::<
            (DrawerState, String, Rc<Vec<u32>>),
        >(__kept);
        Menu(
            ::compose::__private::keep(&__kept.0),
            ::compose::__private::keep(&__kept.1),
            ::compose::__private::keep(&__kept.2),
        )
    };
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        133122779289621940u64,
        __compose_restart,
        ::compose::__private::identity_of(&__compose_restart),
    );
    if !__composer
        .update_kept(
            |__kept: &(DrawerState, String, Rc<Vec<u32>>)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &drawer)
                    || !::compose::__private::ComposeArg::same(&__kept.1, &title)
                    || !::compose::__private::ComposeArg::same(&__kept.2, &shown)
            },
            || (
                ::compose::__private::keep(&drawer),
                ::compose::__private::keep(&title),
                ::compose::__private::keep(&shown),
            ),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    {
        let scope = remember_coroutine_scope();
        IconButton!(
            on_click = move | | { ::compose::__private::marked!(scope, drawer); { scope
            .launch({ #[allow(unused_variables)] let drawer =
            ::compose::__private::reuse(& drawer); async move { drawer.open(). await }
            }); } }, { ::compose::__private::marked!(title); let __compose_region =
            ::compose::__private::region(133122779289621941u64); {
            #[allow(unused_variables)] let title = ::compose::__private::reuse(& title);
            Text!(title) }; }
        );
        LazyColumn!(
            state = drawer, move | list : LazyListScope | {
            ::compose::__private::marked!(shown, title, drawer); { let __compose_region =
            ::compose::__private::region(133122779289621943u64); { list.items(shown
            .len(), { #[allow(unused_variables)] let title =
            ::compose::__private::reuse(& title); #[allow(unused_variables)] let shown =
            ::compose::__private::reuse(& shown); move | index | { let __compose_region =
            ::compose::__private::region(133122779289621942u64); Text!("{title} {}",
            shown[index]) } }); Show(::compose::__private::reuse(& drawer)); } } }
        );
    }
}
#[doc(hidden)]
macro_rules! __compose_call_Menu_6c54f0d88792bace_N {
    (@ path($($path:tt)*) $($arguments:tt)*) => {
        ::compose::__private::call! { Menu[drawer : value, title : value, shown : value]
        plain @ path($($path)*); $($arguments)* }
    };
    ($($arguments:tt)*) => {
        ::compose::__private::call! { Menu[drawer : value, title : value, shown : value]
        plain; $($arguments)* }
    };
}
#[doc(hidden)]
#[allow(unused_imports)]
use __compose_call_Menu_6c54f0d88792bace_N as Menu;
