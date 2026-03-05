use unicode_width::UnicodeWidthStr;

/// Display width of a string, excluding ANSI escape sequences.
pub fn visible_width(s: &str) -> usize {
    anstream::adapter::strip_str(s)
        .map(|fragment| fragment.width())
        .sum()
}

/// Word-wrap `text` to `width`, using `initial_indent` on the first line
/// and `subsequent_indent` on continuation lines.
pub fn wrap_words(
    text: &str,
    out: &mut String,
    width: usize,
    initial_indent: &str,
    subsequent_indent: &str,
) {
    let words: Vec<&str> = text.split_ascii_whitespace().collect();
    if words.is_empty() {
        return;
    }
    out.push_str(initial_indent);
    let mut line_width = visible_width(initial_indent);
    let mut first_on_line = true;
    for word in &words {
        let w = visible_width(word);
        if !first_on_line && line_width + 1 + w > width {
            out.push('\n');
            out.push_str(subsequent_indent);
            line_width = visible_width(subsequent_indent);
            first_on_line = true;
        }
        if !first_on_line {
            out.push(' ');
            line_width += 1;
        }
        out.push_str(word);
        line_width += w;
        first_on_line = false;
    }
    out.push('\n');
}
