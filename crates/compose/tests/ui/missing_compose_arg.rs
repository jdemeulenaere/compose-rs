use compose::runtime::*;

#[composable]
fn Total(values: Vec<i32>, scale: i32) {
    let _ = values.iter().sum::<i32>() * scale;
}

fn main() {}
