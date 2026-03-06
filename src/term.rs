use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Display width of a string, excluding ANSI escape sequences.
pub fn visible_width(s: &str) -> usize {
    anstream::adapter::strip_str(s)
        .map(|fragment| fragment.width())
        .sum()
}

/// Word-wrap `text` to `width`, using `initial_indent` on the first line
/// and `subsequent_indent` on continuation lines. Words wider than the
/// available space are hard-broken at character boundaries.
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
    let subsequent_width = visible_width(subsequent_indent);
    out.push_str(initial_indent);
    let mut line_width = visible_width(initial_indent);
    let mut first_on_line = true;
    for word in &words {
        let w = visible_width(word);
        if !first_on_line && line_width + 1 + w > width {
            out.push('\n');
            out.push_str(subsequent_indent);
            line_width = subsequent_width;
            first_on_line = true;
        }
        if !first_on_line {
            out.push(' ');
            line_width += 1;
        }
        let avail = width.saturating_sub(line_width);
        if w > avail && avail > 0 {
            let subsequent_avail = width.saturating_sub(subsequent_width);
            hard_break_word(word, avail, subsequent_avail, subsequent_indent, out);
            line_width = out
                .rsplit_once('\n')
                .map(|(_, last)| visible_width(last))
                .unwrap_or(visible_width(out));
        } else {
            out.push_str(word);
            line_width += w;
        }
        first_on_line = false;
    }
    out.push('\n');
}

/// Break a word across lines at character boundaries, respecting ANSI escapes.
fn hard_break_word(
    word: &str,
    first_line_avail: usize,
    subsequent_avail: usize,
    indent: &str,
    out: &mut String,
) {
    let mut col = 0;
    let mut avail = first_line_avail;
    let mut chars = word.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            out.push(ch);
            if let Some(&next) = chars.as_str().as_bytes().first() {
                match next {
                    b'[' => {
                        // CSI: \x1b[ ... <letter>
                        for ch in chars.by_ref() {
                            out.push(ch);
                            if ch.is_ascii_alphabetic() {
                                break;
                            }
                        }
                    }
                    b']' => {
                        // OSC: \x1b] ... (ST or BEL)
                        let mut prev = '\0';
                        for ch in chars.by_ref() {
                            out.push(ch);
                            if ch == '\x07' || (prev == '\x1b' && ch == '\\') {
                                break;
                            }
                            prev = ch;
                        }
                    }
                    _ => {
                        // Simple two-char escape
                        if let Some(ch) = chars.next() {
                            out.push(ch);
                        }
                    }
                }
            }
            continue;
        }
        let cw = ch.width().unwrap_or(0);
        if col + cw > avail && col > 0 {
            out.push('\n');
            out.push_str(indent);
            col = 0;
            avail = subsequent_avail;
        }
        out.push(ch);
        col += cw;
    }
}
