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
    let mut styles = ActiveStyles::new();
    for word in &words {
        let w = visible_width(word);
        if !first_on_line && line_width + 1 + w > width {
            styles.emit_line_break(out, subsequent_indent);
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
            hard_break_word(
                word,
                avail,
                subsequent_avail,
                subsequent_indent,
                out,
                &mut styles,
            );
            line_width = out
                .rsplit_once('\n')
                .map(|(_, last)| visible_width(last))
                .unwrap_or(visible_width(out));
        } else {
            out.push_str(word);
            styles.scan(word);
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
    styles: &mut ActiveStyles,
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
                        let start = out.len();
                        for ch in chars.by_ref() {
                            out.push(ch);
                            if ch.is_ascii_alphabetic() {
                                break;
                            }
                        }
                        if out.ends_with('m') {
                            styles.observe_sgr(&out[start - 1..]);
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
            styles.emit_line_break(out, indent);
            col = 0;
            avail = subsequent_avail;
        }
        out.push(ch);
        col += cw;
    }
}

/// Tracks accumulated raw SGR sequences for replay at line breaks.
///
/// Rather than parsing SGR parameters into a structured style, this records the
/// raw escape sequences and replays them verbatim. On full reset (\x1b[0m or
/// \x1b[m), the accumulator is cleared. This approach automatically handles all
/// current and future SGR codes without a fragile parameter parser.
struct ActiveStyles {
    raw: String,
}

impl ActiveStyles {
    fn new() -> Self {
        Self { raw: String::new() }
    }

    fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Record a complete SGR escape sequence (e.g. `\x1b[1m`).
    fn observe_sgr(&mut self, seq: &str) {
        // Extract params: everything between \x1b[ and the trailing m.
        let params = &seq[2..seq.len() - 1];
        if is_full_reset(params) {
            self.raw.clear();
        } else {
            self.raw.push_str(seq);
        }
    }

    /// Scan a string for CSI SGR sequences and record them.
    fn scan(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'\x1b' {
                i += 1;
                continue;
            }
            let seq_start = i;
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    i += 1;
                    while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    if i < bytes.len() {
                        if bytes[i] == b'm' {
                            self.observe_sgr(&s[seq_start..=i]);
                        }
                        i += 1;
                    }
                }
                b']' => {
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == b'\x07' {
                            i += 1;
                            break;
                        }
                        if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
    }

    /// Emit reset (if needed), newline, indent, then restore active styles.
    fn emit_line_break(&self, out: &mut String, indent: &str) {
        if !self.is_empty() {
            out.push_str("\x1b[0m");
        }
        out.push('\n');
        out.push_str(indent);
        out.push_str(&self.raw);
    }
}

fn is_full_reset(params: &str) -> bool {
    params.is_empty() || params == "0"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_break_restores_background() {
        let bg = "\x1b[48;5;235m";
        let reset = "\x1b[0m";
        let word = format!("{bg}abcdefghij{reset}");
        let mut out = String::new();
        let mut styles = ActiveStyles::new();
        hard_break_word(&word, 5, 5, "", &mut out, &mut styles);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert!(line.contains(bg), "missing bg: {line:?}");
            assert!(line.contains(reset), "missing reset: {line:?}");
        }
    }

    #[test]
    fn hard_break_indent_not_styled() {
        let bg = "\x1b[48;5;235m";
        let reset = "\x1b[0m";
        let word = format!("{bg}abcdefghij{reset}");
        let mut out = String::new();
        let mut styles = ActiveStyles::new();
        hard_break_word(&word, 5, 5, "  ", &mut out, &mut styles);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        let line2 = lines[1];
        let indent_end = line2.find(bg).unwrap();
        assert_eq!(&line2[..indent_end], "  ");
    }

    #[test]
    fn wrap_restores_bold_across_break() {
        let text = "\x1b[1maa bb cc\x1b[22m";
        let mut out = String::new();
        wrap_words(text, &mut out, 6, "", "");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() >= 2);
        for line in &lines[1..] {
            if visible_width(line) > 0 {
                assert!(line.contains("\x1b[1m"), "missing bold: {line:?}");
            }
        }
    }

    #[test]
    fn wrap_resets_before_break() {
        let text = "\x1b[48;5;235maa bb\x1b[0m";
        let mut out = String::new();
        wrap_words(text, &mut out, 4, "", "");
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() >= 2);
        assert!(
            lines[0].ends_with("\x1b[0m"),
            "missing reset: {:?}",
            lines[0]
        );
    }

    #[test]
    fn no_style_tracking_when_plain() {
        let mut out = String::new();
        wrap_words("hello world", &mut out, 6, "", "");
        assert!(!out.contains('\x1b'));
    }
}
