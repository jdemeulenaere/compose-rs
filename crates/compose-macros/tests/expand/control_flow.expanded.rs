#[allow(non_snake_case)]
fn Screen(items: Rc<Vec<u32>>, mode: Mode, content: Rc<dyn Fn()>) {
    let __composer = ::compose::__private::current();
    let __compose_group = ::compose::__private::RestartGroup::start(
        __composer,
        5250667602988625983u64,
        |__kept: &dyn ::core::any::Any| {
            let (items, mode, content) = ::compose::__private::kept::<
                (Rc<Vec<u32>>, Mode, Rc<dyn Fn()>),
            >(__kept);
            Screen(items, mode, content)
        },
    );
    if !__composer
        .update_kept(
            |__kept: &(Rc<Vec<u32>>, Mode, Rc<dyn Fn()>)| {
                !::compose::__private::ComposeArg::same(&__kept.0, &items)
                    || !::compose::__private::ComposeArg::same(&__kept.1, &mode)
                    || !::compose::__private::ComposeArg::same(&__kept.2, &content)
            },
            || (
                ::compose::__private::keep(&items),
                ::compose::__private::keep(&mode),
                ::compose::__private::keep(&content),
            ),
        ) && __composer.skipping()
    {
        __composer.skip_to_group_end();
        return;
    }
    if items.is_empty() {
        let __compose_region = ::compose::__private::region(5250667602988625982u64);
        Text("Empty".into());
    } else if items.len() == 1 {
        let __compose_region = ::compose::__private::region(5250667602988625981u64);
        Text("One".into());
    } else {
        log("not a composable call");
    }
    match mode {
        Mode::List => {
            let __compose_region = ::compose::__private::region(5250667602988625979u64);
            {
                {
                    let __compose_region = ::compose::__private::region(
                        5250667602988625980u64,
                    );
                    for item in items.iter() {
                        key(*item, || Row(*item));
                    }
                }
            }
        }
        Mode::Grid => {
            let __compose_region = ::compose::__private::region(5250667602988625978u64);
            Grid(items.clone())
        }
        Mode::None => {}
    }
    {
        let __compose_region = ::compose::__private::region(5250667602988625974u64);
        for item in items.iter() {
            if *item == 0 {
                continue;
            }
            {
                let __compose_region = ::compose::__private::region(
                    5250667602988625977u64,
                );
                Text(item.to_string())
            };
            let doubled = {
                let __compose_region = ::compose::__private::region(
                    5250667602988625976u64,
                );
                Double(*item)
            };
            {
                let __compose_region = ::compose::__private::region(
                    5250667602988625975u64,
                );
                Label!("{doubled}");
            }
        }
    }
    let total: u32 = items.iter().sum();
    let on_click = move || println!("{total}");
    content();
}
