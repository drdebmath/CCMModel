//! Lifting a function out of a crate, and colouring Rust source for the page.

use crate::text::escape;
use std::fmt::Write as _;

/// A function as it stands in its source file.
pub struct Extract {
    /// 1-based line of the first line shown (its doc comment, if any).
    pub line: usize,
    pub text: String,
}

/// Pulls a function, with the doc comment and attributes above it, out of a
/// source file. `name@2` takes the second function of that name.
///
/// rustfmt puts a function's closing brace at the same indentation as its `fn`,
/// which makes this exact where brace counting would be defeated by a brace
/// inside a string literal.
pub fn extract(source: &str, spec: &str) -> Option<Extract> {
    let (name, wanted) = match spec.split_once('@') {
        Some((name, nth)) => (name, nth.parse::<usize>().ok()?),
        None => (spec, 1),
    };
    let lines: Vec<&str> = source.lines().collect();
    let mut doc_start: Option<usize> = None;
    let mut seen = 0;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if trimmed.starts_with("///") || trimmed.starts_with("#[") {
            doc_start.get_or_insert(index);
            index += 1;
            continue;
        }
        let is_fn = ["fn ", "pub fn ", "pub(crate) fn "]
            .iter()
            .any(|prefix| trimmed.starts_with(&format!("{prefix}{name}")));
        let boundary = is_fn
            && trimmed
                .find(&format!("fn {name}"))
                .is_some_and(|at| trimmed[at + 3 + name.len()..].starts_with(['<', '(']));
        if !boundary {
            doc_start = None;
            index += 1;
            continue;
        }
        seen += 1;
        let first = doc_start.take().unwrap_or(index);
        if seen < wanted {
            index += 1;
            continue;
        }
        if trimmed.ends_with('}') && trimmed.matches('{').count() == trimmed.matches('}').count() {
            // A one-line function: `fn f() {}`.
            return Some(Extract {
                line: first + 1,
                text: lines[first..=index].join("\n"),
            });
        }
        let indent = &line[..line.len() - trimmed.len()];
        let closing = format!("{indent}}}");
        for (offset, next) in lines[index..].iter().enumerate() {
            if *next == closing {
                return Some(Extract {
                    line: first + 1,
                    text: lines[first..=index + offset].join("\n"),
                });
            }
        }
        return None;
    }
    None
}

const KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "false", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
    "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while",
];

/// Rust source to HTML, one string per line, with `<span>` classes for
/// keywords, types, function names, strings, numbers, comments and attributes.
/// A token that spans lines is closed and reopened at each line break.
pub fn highlight(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens: Vec<(Option<&str>, String)> = Vec::new();
    let mut i = 0;
    let take = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let start = i;
        let class = if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            Some("com")
        } else if c == '#' && next == Some('[') {
            while i < chars.len() && chars[i] != '\n' && chars[i] != ']' {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            Some("attr")
        } else if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(chars.len());
            Some("str")
        } else if c == '\''
            && (chars.get(i + 2) == Some(&'\'')
                || (next == Some('\\') && chars.get(i + 3) == Some(&'\'')))
        {
            i += if next == Some('\\') { 4 } else { 3 };
            Some("str")
        } else if c.is_ascii_digit() {
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                if chars[i] == '.' && !chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                    break;
                }
                i += 1;
            }
            Some("num")
        } else if c.is_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word = take(start, i);
            if KEYWORDS.contains(&word.as_str()) {
                Some("kw")
            } else if chars.get(i) == Some(&'!') || chars.get(i) == Some(&'(') {
                Some("fn")
            } else if word.starts_with(char::is_uppercase) {
                Some("ty")
            } else {
                None
            }
        } else {
            i += 1;
            None
        };
        let text = take(start, i);
        match tokens.last_mut() {
            Some((None, last)) if class.is_none() => last.push_str(&text),
            _ => tokens.push((class, text)),
        }
    }

    let mut lines = vec![String::new()];
    for (class, text) in tokens {
        for (index, piece) in text.split('\n').enumerate() {
            if index > 0 {
                lines.push(String::new());
            }
            if piece.is_empty() {
                continue;
            }
            let line = lines.last_mut().expect("at least one line");
            match class {
                Some(class) => {
                    let _ = write!(line, "<span class='{class}'>{}</span>", escape(piece));
                }
                None => line.push_str(&escape(piece)),
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "fn run() {}\n\nstruct S;\n\nimpl S {\n    /// Second.\n    fn run(mut self) {\n        let x = \"}\";\n    }\n}\n";

    #[test]
    fn extracts_by_name_and_occurrence() {
        let first = extract(SOURCE, "run").expect("first run");
        assert_eq!((first.line, first.text.as_str()), (1, "fn run() {}"));
        let second = extract(SOURCE, "run@2").expect("second run");
        assert_eq!(second.line, 6);
        assert!(second.text.starts_with("    /// Second.") && second.text.ends_with("    }"));
        assert!(extract(SOURCE, "missing").is_none());
    }

    #[test]
    fn highlights_without_losing_text() {
        let lines = highlight("let s = \"a<b\"; // c\nfn f(x: u8) -> Self { 1 }");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("<span class='kw'>let</span>"));
        assert!(lines[0].contains("<span class='str'>&quot;a&lt;b&quot;</span>"));
        assert!(lines[0].contains("<span class='com'>// c</span>"));
        assert!(lines[1].contains("<span class='fn'>f</span>"));
        assert!(lines[1].contains("<span class='kw'>Self</span>"));
        assert!(lines[1].contains("<span class='num'>1</span>"));
    }
}
