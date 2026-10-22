//! Keeps the guide's quotes in sync with the samples. In `docs/guide/*.md`, a line
//! `<!-- snippet: src/state.rs#counter -->` before a fenced block says the block quotes the region
//! between `// ANCHOR: counter` and `// ANCHOR_END: counter` in `examples/guide/src/state.rs`
//! (a path without `#name` quotes the whole file). This test fails when a quote differs from its
//! region, a region isn't quoted, or a `rust` block has no marker (so every Rust sample in the
//! guide compiles); `UPDATE_GUIDE=1 cargo test -p guide snippets` rewrites the quotes from the
//! sources.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const MARKER: &str = "<!-- snippet: ";

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn guide_dir() -> PathBuf {
    crate_dir().join("../../docs/guide")
}

/// The regions of a source file, by name: their lines without the anchors (nested ones too),
/// less their common indentation.
fn regions(text: &str) -> BTreeMap<String, String> {
    let mut open: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut done = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(name) = trimmed.strip_prefix("// ANCHOR: ") {
            open.insert(name.trim().to_string(), Vec::new());
        } else if let Some(name) = trimmed.strip_prefix("// ANCHOR_END: ") {
            let lines = open.remove(name.trim()).unwrap_or_else(|| panic!("no ANCHOR: {name}"));
            done.insert(name.trim().to_string(), dedent(&lines));
        } else {
            for lines in open.values_mut() {
                lines.push(line);
            }
        }
    }
    assert!(open.is_empty(), "unclosed anchors: {:?}", open.keys());
    done
}

fn dedent(lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out = String::new();
    for line in lines {
        out.push_str(line.get(indent..).unwrap_or("").trim_end());
        out.push('\n');
    }
    out
}

/// Every anchored region of the guide crate's sources, as `path#name`.
fn all_regions() -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for directory in ["src", "tests/suite"] {
        let mut paths: Vec<_> = fs::read_dir(crate_dir().join(directory))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
            .collect();
        paths.sort();
        for path in paths {
            let text = fs::read_to_string(&path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            for (region, body) in regions(&text) {
                found.insert(format!("{directory}/{name}#{region}"), body);
            }
        }
    }
    found
}

/// What a marker quotes.
fn quoted(reference: &str, regions: &BTreeMap<String, String>) -> String {
    if reference.contains('#') {
        regions.get(reference).cloned().unwrap_or_else(|| panic!("no region {reference}"))
    } else {
        fs::read_to_string(crate_dir().join(reference))
            .unwrap_or_else(|error| panic!("{reference}: {error}"))
    }
}

/// Checks (or rewrites) one guide page; returns the references it quotes and its mismatches.
fn check_page(
    path: &Path,
    regions: &BTreeMap<String, String>,
    update: bool,
) -> (Vec<String>, Vec<String>) {
    let text = fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let (mut out, mut references, mut mismatches) = (Vec::new(), Vec::new(), Vec::new());
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        out.push(line.to_string());
        index += 1;
        let Some(reference) = line.strip_prefix(MARKER).and_then(|rest| rest.strip_suffix(" -->"))
        else {
            continue;
        };
        let fence = lines.get(index).copied().unwrap_or("");
        assert!(fence.starts_with("```"), "{}: a fenced block after {line}", path.display());
        out.push(fence.to_string());
        index += 1;
        let start = index;
        while index < lines.len() && lines[index] != "```" {
            index += 1;
        }
        let mut block: String =
            lines[start..index].iter().map(|line| format!("{line}\n")).collect();
        let expected = quoted(reference, regions);
        if block != expected {
            mismatches.push(format!(
                "{}: {reference}\n--- quoted:\n{block}--- source:\n{expected}",
                path.display()
            ));
            block = expected;
        }
        out.extend(block.lines().map(str::to_string));
        references.push(reference.to_string());
    }
    if update {
        let mut new = out.join("\n");
        new.push('\n');
        if new != text {
            fs::write(path, new).unwrap();
        }
    }
    (references, mismatches)
}

/// The lines (1-based) of `rust` blocks without a marker before them.
fn unmarked_rust_blocks(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut unmarked = Vec::new();
    let mut in_block = false;
    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("```") {
            continue;
        }
        if in_block {
            in_block = false;
            continue;
        }
        in_block = true;
        let marked = index > 0 && lines[index - 1].starts_with(MARKER);
        if line.starts_with("```rust") && !marked {
            unmarked.push(index + 1);
        }
    }
    unmarked
}

#[test]
fn every_rust_block_quotes_a_compiled_sample() {
    let mut unmarked = Vec::new();
    for entry in fs::read_dir(guide_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "md") {
            let text = fs::read_to_string(&path).unwrap();
            for line in unmarked_rust_blocks(&text) {
                unmarked.push(format!("{}:{line}", path.display()));
            }
        }
    }
    assert!(unmarked.is_empty(), "`rust` blocks without a snippet marker: {unmarked:?}");
    assert_eq!(unmarked_rust_blocks("```kotlin\nx\n```\n```rust\ny\n```\n"), [4]);
}

#[test]
fn the_guide_quotes_the_samples_as_they_are() {
    let regions = all_regions();
    let update = std::env::var_os("UPDATE_GUIDE").is_some();
    let mut pages: Vec<_> = fs::read_dir(guide_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    pages.sort();
    let (mut quoted, mut mismatches) = (BTreeSet::new(), Vec::new());
    for page in &pages {
        let (references, page_mismatches) = check_page(page, &regions, update);
        quoted.extend(references);
        mismatches.extend(page_mismatches);
    }
    if !update {
        assert!(
            mismatches.is_empty(),
            "quotes out of date (UPDATE_GUIDE=1 rewrites them):\n{}",
            mismatches.join("\n")
        );
    }
    let unquoted: Vec<_> = regions.keys().filter(|region| !quoted.contains(*region)).collect();
    assert!(unquoted.is_empty(), "regions the guide doesn't quote: {unquoted:?}");
}

#[test]
fn regions_are_read_without_their_anchors_or_indentation() {
    let text = "fn a() {\n    // ANCHOR: outer\n    let x = 1;\n    // ANCHOR: inner\n    if x > 0 {\n        x;\n    }\n    // ANCHOR_END: inner\n    // ANCHOR_END: outer\n}\n";
    let regions = regions(text);
    assert_eq!(regions["outer"], "let x = 1;\nif x > 0 {\n    x;\n}\n");
    assert_eq!(regions["inner"], "if x > 0 {\n    x;\n}\n");
}
