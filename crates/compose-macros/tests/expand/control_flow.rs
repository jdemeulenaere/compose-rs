#[composable]
fn Screen(items: Rc<Vec<u32>>, mode: Mode, content: Rc<dyn Fn()>) {
    if items.is_empty() {
        Text("Empty".into());
    } else if items.len() == 1 {
        Text("One".into());
    } else {
        log("not a composable call");
    }
    match mode {
        Mode::List => {
            for item in items.iter() {
                key(*item, || Row(*item));
            }
        }
        Mode::Grid => Grid(items.clone()),
        Mode::None => {}
    }
    for item in items.iter() {
        if *item == 0 {
            continue;
        }
        Text(item.to_string());
        let doubled = Double(*item);
        Label!("{doubled}");
    }
    let total: u32 = items.iter().sum();
    let on_click = move || println!("{total}");
    content();
}
