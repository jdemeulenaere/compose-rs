//! Event scripts: a compact text format for app sessions (docs/09-testing.md), one command per
//! line, `#` starting a comment:
//!
//! ```text
//! click "Count: 0"        # clicks the node whose text is exactly "Count: 0"
//! click tag:reset         # clicks the node tagged "reset"
//! type tag:draft "milk"   # types into the node tagged "draft"
//! type "milk"             # types into the focused node
//! key Tab                 # presses and releases a key: Enter, Space, Tab, Backspace, …
//! assert "Count: 1"       # a node with this text exists
//! assert_not "Reset"      # none does
//! frame                   # runs frames until idle
//! dump                    # appends the node tree
//! semantics               # appends the merged semantics tree
//! display                 # appends the display list
//! ```
//!
//! Running a script returns its transcript: each command, then what it printed. Tests compare
//! it with an expected file.

use std::fs;
use std::path::Path;

use compose_ui::Key;

use crate::{ComposeTestRule, Matcher, has_test_tag, has_text};

/// Runs `script` against `rule`, returning the transcript. Panics with the line number on an
/// unknown command or a failed action.
pub fn run_script(rule: &mut ComposeTestRule, script: &str) -> String {
    let mut out = String::new();
    for (number, line) in script.lines().enumerate() {
        let command = strip_comment(line).trim();
        if command.is_empty() {
            continue;
        }
        out.push_str("> ");
        out.push_str(command);
        out.push('\n');
        let words =
            split(command).unwrap_or_else(|error| panic!("script line {}: {error}", number + 1));
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(rule, &words, &mut out)));
        if let Err(panic) = result {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_default();
            panic!(
                "script line {} (`{command}`) failed: {message}\ntranscript so far:\n{out}",
                number + 1
            );
        }
    }
    out
}

/// Runs the script at `path` and compares its transcript with `path` + `.expected`, or writes
/// that file when `UPDATE_SCRIPTS` is set.
#[track_caller]
pub fn check_script(rule: &mut ComposeTestRule, path: impl AsRef<Path>) {
    let path = path.as_ref();
    let script = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    let transcript = run_script(rule, &script);
    let expected_path = path.with_extension(format!(
        "{}.expected",
        path.extension().and_then(|extension| extension.to_str()).unwrap_or("script")
    ));
    if std::env::var_os("UPDATE_SCRIPTS").is_some() {
        fs::write(&expected_path, &transcript).expect("write the expected transcript");
        return;
    }
    let expected = fs::read_to_string(&expected_path).unwrap_or_default();
    assert!(
        expected == transcript,
        "the transcript of {} differs from {} (UPDATE_SCRIPTS=1 to accept):\n--- expected\n{expected}\n--- actual\n{transcript}",
        path.display(),
        expected_path.display()
    );
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, c) in line.char_indices() {
        match c {
            '\\' if quoted && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !escaped => quoted = !quoted,
            '#' if !quoted => return &line[..index],
            _ => {}
        }
        escaped = false;
    }
    line
}

/// A word: bare, or quoted with `\"`, `\\` and `\n` escapes.
#[derive(Debug, PartialEq)]
enum Word {
    Bare(String),
    Quoted(String),
}

fn split(command: &str) -> Result<Vec<Word>, String> {
    let mut words = Vec::new();
    let mut chars = command.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let mut text = String::new();
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some('n') => text.push('\n'),
                        Some(other) => text.push(other),
                        None => return Err("unfinished escape".into()),
                    },
                    Some(other) => text.push(other),
                    None => return Err("unfinished string".into()),
                }
            }
            words.push(Word::Quoted(text));
        } else {
            let mut text = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                text.push(c);
                chars.next();
            }
            words.push(Word::Bare(text));
        }
    }
    Ok(words)
}

/// A node reference: `"text"` or `tag:name`.
fn target(word: &Word) -> Matcher {
    match word {
        Word::Quoted(text) => has_text(text),
        Word::Bare(bare) => match bare.strip_prefix("tag:") {
            Some(tag) => has_test_tag(tag),
            None => panic!("expected \"text\" or tag:name, found {bare}"),
        },
    }
}

fn key(name: &str) -> Key {
    match name {
        "Enter" => Key::Enter,
        "Space" => Key::Space,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Escape" => Key::Escape,
        "Left" => Key::DirectionLeft,
        "Right" => Key::DirectionRight,
        "Up" => Key::DirectionUp,
        "Down" => Key::DirectionDown,
        "Home" => Key::MoveHome,
        "End" => Key::MoveEnd,
        single if single.chars().count() == 1 => {
            Key::Character(single.chars().next().expect("a char"))
        }
        other => panic!("unknown key {other}"),
    }
}

fn run(rule: &mut ComposeTestRule, words: &[Word], out: &mut String) {
    let Some(Word::Bare(command)) = words.first() else { panic!("expected a command") };
    let arguments = &words[1..];
    match (command.as_str(), arguments) {
        ("click", [node]) => {
            rule.on_node(target(node)).perform_click();
        }
        ("type", [Word::Quoted(text)]) => rule.type_text(text),
        ("type", [node, Word::Quoted(text)]) => {
            rule.on_node(target(node)).perform_text_input(text);
        }
        ("key", [Word::Bare(name)]) => rule.press_key(key(name)),
        ("assert", [node]) => {
            rule.on_node(target(node)).assert_exists();
        }
        ("assert_not", [node]) => {
            rule.on_node(target(node)).assert_does_not_exist();
        }
        ("frame", []) => rule.wait_for_idle(),
        ("dump", []) => out.push_str(&rule.dump_tree()),
        ("semantics", []) => out.push_str(&rule.dump_semantics()),
        ("display", []) => out.push_str(&rule.dump_display_list()),
        (other, _) => panic!("unknown command or arguments: {other} {arguments:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_and_strips_comments() {
        assert_eq!(strip_comment(r#"click "a # b" # comment"#), r#"click "a # b" "#);
        assert_eq!(
            split(r#"type tag:draft "say \"hi\"\n""#).unwrap(),
            [
                Word::Bare("type".into()),
                Word::Bare("tag:draft".into()),
                Word::Quoted("say \"hi\"\n".into())
            ]
        );
    }
}
