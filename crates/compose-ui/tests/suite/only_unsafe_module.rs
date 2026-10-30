//! compose-ui's `unsafe` is one module's (D1211): `deny(unsafe_code)` would allow a later `#[allow]`,
//! and the design's condition for having any is that it's in one place, under Miri.

use std::fs;
use std::path::Path;

fn sources(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).expect("the crate's sources") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn only_tree_access_and_its_allow_contain_unsafe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(files.len() > 20, "the sources weren't found");
    let mut found = Vec::new();
    for file in files {
        let name = file.strip_prefix(&root).expect("under it").display().to_string();
        for (number, line) in fs::read_to_string(&file).expect("a source").lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if !code.contains("unsafe") {
                continue;
            }
            let allowed = name == "tree_access.rs"
                || (name == "lib.rs"
                    && (code.contains("#![deny(unsafe_code)]")
                        || code.contains("#[allow(unsafe_code)]")));
            if !allowed {
                found.push(format!("{name}:{}: {line}", number + 1));
            }
        }
    }
    assert!(found.is_empty(), "`unsafe` outside tree_access.rs: {found:#?}");
    let lib = fs::read_to_string(root.join("lib.rs")).expect("lib.rs");
    assert_eq!(lib.matches("#[allow(unsafe_code)]").count(), 1, "one `allow`");
    let next = lib.split("#[allow(unsafe_code)]").nth(1).expect("the allow");
    assert!(next.trim_start().starts_with("mod tree_access;"), "on the window's module");
}
