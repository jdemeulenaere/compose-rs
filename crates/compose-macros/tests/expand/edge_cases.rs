#[composable]
fn Filtered(ids: Rc<Vec<u32>>, on_change: Rc<dyn Fn(&str)>) {
    #![allow(unused_variables)]
    for &id in ids.iter() {
        if id == 0 {
            continue;
        }
        key(id, || Row(id));
    }
    let Some(first) = ids.first() else {
        Row(0);
        return;
    };
    let task = async move {
        if *first > 0 {
            Event::Clicked(*first).send().await;
        }
    };
}

#[composable]
fn Conditional(#[cfg(feature = "extra")] extra: u32, (ref mut count, step): (u32, u32)) {
    *count += step;
}
