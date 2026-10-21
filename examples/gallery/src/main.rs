use compose::desktop::{WindowOptions, run_app};
use compose::ui::LayoutDirection;

fn main() {
    // `GALLERY_ABOUT=1` opens the "About" dialog from the start; `GALLERY_RTL=1` lays the
    // gallery out right to left.
    let layout_direction = if std::env::var_os("GALLERY_RTL").is_some() {
        LayoutDirection::Rtl
    } else {
        LayoutDirection::Ltr
    };
    run_app(
        WindowOptions {
            title: "Gallery".into(),
            size: (420, 760).into(),
            layout_direction,
            ..Default::default()
        },
        move || gallery::GalleryWith(std::env::var_os("GALLERY_ABOUT").is_some()),
    );
}
