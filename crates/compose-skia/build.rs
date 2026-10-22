//! On Linux, Skia links the system's FreeType and Fontconfig (skia-bindings asks for
//! `-lfreetype -lfontconfig`). Without their development packages the final link fails with
//! "unable to find library -lfreetype": say what to install before it does.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    let missing: Vec<&str> = ["freetype2", "fontconfig"]
        .into_iter()
        .filter(|library| pkg_config::Config::new().cargo_metadata(false).probe(library).is_err())
        .collect();
    if !missing.is_empty() {
        println!(
            "cargo:warning=compose-skia: pkg-config can't find {}. Skia links the system's \
             FreeType and Fontconfig: install their development packages, e.g. \
             `sudo apt install pkg-config libfreetype-dev libfontconfig1-dev` (Debian, Ubuntu), \
             `sudo dnf install pkgconf-pkg-config freetype-devel fontconfig-devel` (Fedora) or \
             `sudo pacman -S pkgconf freetype2 fontconfig` (Arch).",
            missing.join(" and ")
        );
    }
}
