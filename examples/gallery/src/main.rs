use compose::desktop::{WindowOptions, run_app};

fn main() {
    // `GALLERY_ABOUT=1` opens the "About" dialog from the start.
    run_app(
        WindowOptions { title: "Gallery".into(), size: (420, 760).into(), ..Default::default() },
        move || gallery::GalleryWith(std::env::var_os("GALLERY_ABOUT").is_some()),
    );
}
