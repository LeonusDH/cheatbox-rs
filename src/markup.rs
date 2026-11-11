//! Minimal subset of `rich` console markup: `[bold red]text[/]`, `[on blue]`,
//! `[#ff8800]`, `[color(208)]`, `[rgb(1,2,3)]`, closing `[/]` / `[/name]`,
//! and `\[` escapes.
//!
//! Unlike `rich`, a bracketed word that is NOT a valid style (e.g. `[pod]`
//! in `kubectl logs [pod]`) is printed literally instead of silently vanishing.

use crate::colors::NAMED;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Idx(u8),
    Rgb(u8, u8, u8),
    Default,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub reverse: Option<bool>,
}

impl Style {
    pub fn fg(c: Color) -> Self {
        Style { fg: Some(c), ..Default::default() }
    }

    /// `other` wins wherever it sets something.
    pub fn over(self, other: Style) -> Style {
        Style {
            fg: other.fg.or(self.fg),
            bg: other.bg.or(self.bg),
            bold: other.bold.or(self.bold),
            dim: other.dim.or(self.dim),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            strike: other.strike.or(self.strike),
            reverse: other.reverse.or(self.reverse),
        }
    }

    pub fn parse(s: &str) -> Option<Style> {
        let mut st = Style::default();
        let mut words = s.split_whitespace().map(|w| w.to_ascii_lowercase());
        let mut any = false;
        while let Some(w) = words.next() {
            any = true;
            match w.as_str() {
                "on" => st.bg = Some(parse_color(&words.next()?)?),
                "not" => {
                    let a = words.next()?;
                    set_attr(&mut st, &a, false)?;
                }
                _ => {
                    if set_attr(&mut st, &w, true).is_none() {
                        st.fg = Some(parse_color(&w)?);
                    }
                }
            }
        }
        any.then_some(st)
    }

    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }

    pub fn sgr(&self) -> String {
        let mut codes: Vec<String> = Vec::new();
        let flag = |v: Option<bool>, on: &str, codes: &mut Vec<String>| {
            if v == Some(true) {
                codes.push(on.into());
            }
        };
        flag(self.bold, "1", &mut codes);
        flag(self.dim, "2", &mut codes);
        flag(self.italic, "3", &mut codes);
        flag(self.underline, "4", &mut codes);
        flag(self.reverse, "7", &mut codes);
        flag(self.strike, "9", &mut codes);
        if let Some(c) = self.fg {
            codes.push(color_code(c, false));
        }
        if let Some(c) = self.bg {
            codes.push(color_code(c, true));
        }
        if codes.is_empty() {
            String::new()
        } else {
            format!("\x1b[{}m", codes.join(";"))
        }
    }
}

fn set_attr(st: &mut Style, w: &str, v: bool) -> Option<()> {
    match w {
        "bold" | "b" => st.bold = Some(v),
        "dim" | "d" => st.dim = Some(v),
        "italic" | "i" => st.italic = Some(v),
        "underline" | "u" => st.underline = Some(v),
        "strike" | "s" => st.strike = Some(v),
        "reverse" | "r" => st.reverse = Some(v),
        _ => return None,
    }
    Some(())
}

fn color_code(c: Color, bg: bool) -> String {
    let base = if bg { 40 } else { 30 };
    match c {
        Color::Default => format!("{}", base + 9),
        Color::Idx(i) if i < 8 => format!("{}", base + i as u32),
        Color::Idx(i) if i < 16 => format!("{}", base + 60 + (i as u32 - 8)),
        Color::Idx(i) => format!("{};5;{}", base + 8, i),
        Color::Rgb(r, g, b) => format!("{};2;{};{};{}", base + 8, r, g, b),
    }
}

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_ascii_lowercase();
    if s == "default" {
        return Some(Color::Default);
    }
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let v = u32::from_str_radix(hex, 16).ok()?;
            return Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
        }
        return None;
    }
    if let Some(n) = s.strip_prefix("color(").and_then(|r| r.strip_suffix(')')) {
        return n.trim().parse::<u8>().ok().map(Color::Idx);
    }
    if let Some(n) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let p: Vec<u8> = n.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        return (p.len() == 3).then(|| Color::Rgb(p[0], p[1], p[2]));
    }
    NAMED
        .binary_search_by(|(k, _)| (*k).cmp(s.as_str()))
        .ok()
        .map(|i| Color::Idx(NAMED[i].1))
}

/// One styled character cell.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

/// A line of styled text.
pub type Line = Vec<Cell>;

pub fn width(line: &[Cell]) -> usize {
    line.iter().map(|c| c.ch.width().unwrap_or(0)).sum()
}

pub fn plain(s: &str, style: Style) -> Line {
    s.chars().map(|ch| Cell { ch, style }).collect()
}

/// Parse markup into a styled line. Newlines are kept as cells; callers split.
pub fn parse(src: &str, base: Style) -> Line {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut stack: Vec<(String, Style)> = Vec::new();
    let cur = |stack: &Vec<(String, Style)>| stack.iter().fold(base, |a, (_, s)| a.over(*s));
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1) == Some(&'[') {
            out.push(Cell { ch: '[', style: cur(&stack) });
            i += 2;
            continue;
        }
        if c == '[' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == ']' || x == '[') {
                let end = i + 1 + end;
                if chars[end] == ']' {
                    let tag: String = chars[i + 1..end].iter().collect();
                    if let Some(name) = tag.strip_prefix('/') {
                        let name = name.trim();
                        let pos = if name.is_empty() {
                            stack.len().checked_sub(1)
                        } else {
                            stack.iter().rposition(|(n, _)| n == name)
                        };
                        if let Some(p) = pos {
                            stack.truncate(p);
                            i = end + 1;
                            continue;
                        }
                    } else if let Some(st) = Style::parse(&tag) {
                        stack.push((tag.trim().to_string(), st));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        out.push(Cell { ch: c, style: cur(&stack) });
        i += 1;
    }
    out
}

/// Word-wrap a styled line to `w` columns (long words are folded).
pub fn wrap(line: &[Cell], w: usize) -> Vec<Line> {
    let w = w.max(1);
    let mut lines: Vec<Line> = Vec::new();
    let mut cur: Line = Vec::new();
    let mut cur_w = 0;
    // split into words separated by spaces
    let mut words: Vec<Line> = Vec::new();
    let mut word: Line = Vec::new();
    for &c in line {
        if c.ch == ' ' {
            words.push(std::mem::take(&mut word));
        } else {
            word.push(c);
        }
    }
    words.push(word);

    for word in words {
        let ww = width(&word);
        let sep = if cur.is_empty() { 0 } else { 1 };
        if cur_w + sep + ww <= w {
            if sep == 1 {
                cur.push(Cell { ch: ' ', style: word.first().map(|c| c.style).unwrap_or_default() });
            }
            cur_w += sep + ww;
            cur.extend(word);
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        // fold words longer than the line
        for c in word {
            let cw = c.ch.width().unwrap_or(0);
            if cur_w + cw > w && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            cur.push(c);
            cur_w += cw;
        }
    }
    lines.push(cur);
    lines
}

pub fn pad_right(mut line: Line, w: usize) -> Line {
    let cw = width(&line);
    line.extend(std::iter::repeat_n(Cell { ch: ' ', style: Style::default() }, w.saturating_sub(cw)));
    line
}

pub fn pad_left(line: Line, w: usize) -> Line {
    let cw = width(&line);
    let mut out: Line = std::iter::repeat_n(Cell { ch: ' ', style: Style::default() }, w.saturating_sub(cw)).collect();
    out.extend(line);
    out
}

pub fn to_string(line: &[Cell], color: bool) -> String {
    let mut s = String::new();
    let mut last: Option<Style> = None;
    for c in line {
        if color && last != Some(c.style) {
            if last.is_some_and(|l| !l.is_plain()) {
                s.push_str("\x1b[0m");
            }
            s.push_str(&c.style.sgr());
            last = Some(c.style);
        }
        s.push(c.ch);
    }
    if color && last.is_some_and(|l| !l.is_plain()) {
        s.push_str("\x1b[0m");
    }
    s.trim_end_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &[Cell]) -> String {
        l.iter().map(|c| c.ch).collect()
    }

    #[test]
    fn styles_and_close() {
        let l = parse("[bold black]a[white]b[/]c[/]d", Style::default());
        assert_eq!(text(&l), "abcd");
        assert_eq!(l[0].style.fg, Some(Color::Idx(0)));
        assert_eq!(l[1].style.fg, Some(Color::Idx(7)));
        assert_eq!(l[1].style.bold, Some(true));
        assert_eq!(l[2].style.fg, Some(Color::Idx(0)));
        assert!(l[3].style.is_plain());
    }

    #[test]
    fn placeholders_survive() {
        let l = parse("kubectl cp [file] [pod]:[path]", Style::default());
        assert_eq!(text(&l), "kubectl cp [file] [pod]:[path]");
    }

    #[test]
    fn escape_and_named() {
        assert_eq!(text(&parse(r"\[red]x", Style::default())), "[red]x");
        assert!(parse_color("misty_rose3").is_some());
        assert!(parse_color("navajo_white1").is_some());
        assert_eq!(parse_color("#ff0080"), Some(Color::Rgb(255, 0, 128)));
    }

    #[test]
    fn wrapping() {
        let l = plain("Create an empty file or update file timestamps", Style::default());
        let w = wrap(&l, 20);
        assert!(w.iter().all(|x| width(x) <= 20));
        assert_eq!(w.len(), 3);
    }
}
