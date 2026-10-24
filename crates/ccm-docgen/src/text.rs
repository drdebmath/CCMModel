//! Escaping, a little inline Markdown, and a JSON writer — enough for the page
//! without pulling in a dependency.

use std::fmt::Write as _;

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Inline Markdown to HTML: `code`, **bold**, *emphasis* and [text](url).
/// Everything else is escaped.
pub fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                let _ = write!(out, "<code>{}</code>", escape(&after[..end]));
                rest = &after[end + 1..];
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                let _ = write!(out, "<strong>{}</strong>", inline(&after[..end]));
                rest = &after[end + 2..];
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('*') {
            if let Some(end) = after.find('*') {
                let _ = write!(out, "<em>{}</em>", inline(&after[..end]));
                rest = &after[end + 1..];
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('[') {
            if let Some(mid) = after.find("](") {
                if let Some(end) = after[mid..].find(')') {
                    let label = &after[..mid];
                    let url = &after[mid + 2..mid + end];
                    let _ = write!(out, r#"<a href="{}">{}</a>"#, escape(url), inline(label));
                    rest = &after[mid + end + 1..];
                    continue;
                }
            }
        }
        out.push_str(&escape(&c.to_string()));
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Paragraphs to HTML; consecutive entries starting with "- " become a list.
pub fn paragraphs(entries: &[&str]) -> String {
    let mut out = String::new();
    let mut in_list = false;
    for entry in entries {
        if let Some(item) = entry.strip_prefix("- ") {
            if !in_list {
                out.push_str("<ul>");
                in_list = true;
            }
            let _ = write!(out, "<li>{}</li>", inline(item));
        } else {
            if in_list {
                out.push_str("</ul>");
                in_list = false;
            }
            let _ = write!(out, "<p>{}</p>", inline(entry));
        }
    }
    if in_list {
        out.push_str("</ul>");
    }
    out
}

/// A JSON string literal. `</` is written `<\/` so the data can sit in a
/// `<script>` without ending it.
pub fn json_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '/' if out.ends_with('<') => out.push_str("\\/"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A JSON array of strings.
pub fn json_strings<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let parts: Vec<String> = items.into_iter().map(json_str).collect();
    format!("[{}]", parts.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_markdown_is_escaped_and_marked_up() {
        assert_eq!(
            inline("a `x<y` **b** *c* [d](e.html) <f>"),
            "a <code>x&lt;y</code> <strong>b</strong> <em>c</em> <a href=\"e.html\">d</a> &lt;f&gt;"
        );
    }

    #[test]
    fn json_strings_are_safe_in_a_script() {
        assert_eq!(json_str("a\"b\n</script>"), "\"a\\\"b\\n<\\/script>\"");
    }
}
