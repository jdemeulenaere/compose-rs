//! The text plot Compose's `MonoSplineTest` and `ArcAnimationTest` compare curves with.

/// `y` by `x` in `columns` by `rows` of characters, as Compose's `drawTextGraph`, not flipped.
pub(crate) fn draw_text_graph(columns: usize, rows: usize, x: &[f32], y: &[f32]) -> String {
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (x[0], x[0], y[0], y[0]);
    for (x, y) in x.iter().zip(y) {
        min_x = min_x.min(*x);
        max_x = max_x.max(*x);
        min_y = min_y.min(*y);
        max_y = max_y.max(*y);
    }
    let mut grid = vec![vec![' '; columns]; rows];
    for (x, y) in x.iter().zip(y) {
        let column = ((columns - 1) as f32 * (x - min_x) / (max_x - min_x)) as usize;
        let row = ((rows - 1) as f32 * (y - min_y) / (max_y - min_y)) as usize;
        grid[row][column] = '*';
    }
    let rounded = |value: f32| (f64::from(value * 1000.0) + 0.5).trunc() as i32 as f32 / 1000.0;
    let mut text = String::new();
    for (index, row) in grid.iter().enumerate() {
        let line: String = row.iter().collect();
        if index % 5 == 0 || index == rows - 1 {
            let value = (max_y - min_y) * (index as f32 / (rows as f32 - 1.0)) + min_y;
            text += &format!("|{line}| {:?}\n", rounded(value));
        } else {
            text += &format!("|{line}|\n");
        }
    }
    let (min, max) = (format!("{:?}", rounded(min_x)), format!("{:?}", rounded(max_x)));
    let footer = format!("{min}{}", " ".repeat(columns));
    text + &footer[..columns - max.len() + 2] + &max
}
