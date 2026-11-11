//! Bento layout: ASCII header (logo + title) and two masonry columns of
//! rounded panels. Heights are computed from the real wrapped rows, so
//! columns always line up (the Python original mis-aligns when a
//! description wraps).

use crate::markup::{self, parse, parse_color, plain, Cell, Color, Line, Style};
use crate::sheet::{Category, Sheet};

pub struct Opts {
    pub width: usize,
    pub header: bool,
}

const MIN_DESC: usize = 24;

fn primary(sheet: &Sheet) -> Style {
    Style::fg(parse_color(&sheet.primary_color).unwrap_or(Color::Idx(4)))
}

fn spaces(n: usize) -> Line {
    plain(&" ".repeat(n), Style::default())
}

fn split_lines(l: Line) -> Vec<Line> {
    let mut out = vec![Vec::new()];
    for c in l {
        if c.ch == '\n' {
            out.push(Vec::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
    }
    out
}

/// Command column width for one box: as wide as its longest command,
/// but always leaving room for descriptions.
fn command_col(cats: &[&Category], inner: usize) -> usize {
    let longest = cats
        .iter()
        .flat_map(|c| c.entries.iter())
        .map(|e| markup::width(&parse(&e.command, Style::default())))
        .max()
        .unwrap_or(0);
    let want = longest.max("Command".len());
    want.min(inner.saturating_sub(MIN_DESC + 2)).max(7)
}

/// Render one panel; `extra` blank rows are appended to stretch it.
fn panel(cat: &Category, width: usize, prim: Style, extra: usize) -> Vec<Line> {
    let inner = width.saturating_sub(4);
    let cmd_w = command_col(&[cat], inner);
    let desc_w = inner.saturating_sub(cmd_w + 2).max(1);
    let border = |s: &str| plain(s, prim);

    // top border with title
    let mut title = parse(&cat.name, prim);
    let max_title = width.saturating_sub(6);
    if markup::width(&title) > max_title {
        title = markup::wrap(&title, max_title).remove(0);
    }
    let mut top = border("╭─ ");
    let tw = markup::width(&title);
    top.extend(title);
    top.extend(border(" "));
    top.extend(border(&"─".repeat(width.saturating_sub(tw + 5))));
    top.extend(border("╮"));

    let mut rows: Vec<Line> = Vec::new();
    let head = Style { bold: Some(true), dim: Some(true), ..Default::default() };
    let mut h = markup::pad_left(plain("Command", head), cmd_w);
    h.extend(spaces(2));
    h.extend(plain("Description", head));
    rows.push(h);

    for e in &cat.entries {
        let cmd = markup::wrap(&parse(&e.command, prim), cmd_w);
        let desc = markup::wrap(&parse(&e.description, Style::default()), desc_w);
        for i in 0..cmd.len().max(desc.len()) {
            let mut r = markup::pad_left(cmd.get(i).cloned().unwrap_or_default(), cmd_w);
            r.extend(spaces(2));
            r.extend(desc.get(i).cloned().unwrap_or_default());
            rows.push(r);
        }
    }
    rows.extend(std::iter::repeat_n(Vec::new(), extra));

    let mut out = vec![top];
    for r in rows {
        let mut l = border("│ ");
        l.extend(markup::pad_right(r, inner));
        l.extend(border(" │"));
        out.push(l);
    }
    out.push(border(&format!("╰{}╯", "─".repeat(width.saturating_sub(2)))));
    out
}

fn header(sheet: &Sheet, width: usize) -> Vec<Line> {
    // Parse the whole block at once (like rich): a tag may open on one line
    // and close on another.
    let logo: Vec<Line> = if sheet.logo.is_empty() {
        Vec::new()
    } else {
        split_lines(parse(&sheet.logo.join("\n"), Style::default()))
    };
    let mut title: Vec<Line> = if sheet.title.is_empty() {
        Vec::new()
    } else {
        split_lines(parse(&sheet.title.join("\n"), Style::default()))
    };
    let lw = logo.iter().map(|l| markup::width(l)).max().unwrap_or(0);
    let tw = title.iter().map(|l| markup::width(l)).max().unwrap_or(0);
    if logo.is_empty() && title.is_empty() {
        return Vec::new();
    }
    // subtitle under the title, centred like rich's panel subtitle
    let sub = "CheatBox";
    let sw = tw.max(sub.len());
    let mut s = spaces((sw - sub.len()) / 2);
    s.extend(plain(sub, Style { dim: Some(true), ..Default::default() }));
    title.push(s);

    let mut left_w = lw + 6; // 2 indent + logo + 4 gap
    let (logo, lw) = if left_w + sw + 2 > width {
        left_w = 0;
        (Vec::new(), 0) // not enough room: drop the logo
    } else {
        (logo, lw)
    };
    if sw + 2 > width {
        return Vec::new();
    }
    let _ = lw;
    let rows = logo.len().max(title.len());
    let mut out = vec![Vec::new()];
    for i in 0..rows {
        let mut l = Vec::new();
        if left_w > 0 {
            l.extend(spaces(2));
            l.extend(markup::pad_right(logo.get(i).cloned().unwrap_or_default(), left_w - 2));
        }
        l.extend(spaces(2));
        l.extend(title.get(i).cloned().unwrap_or_default());
        out.push(l);
    }
    out.push(Vec::new());
    out
}

pub fn render(sheet: &Sheet, opts: &Opts) -> Vec<Line> {
    let width = opts.width.max(30);
    let prim = primary(sheet);
    let mut out = if opts.header { header(sheet, width) } else { Vec::new() };

    let col2 = (width - 1) / 2;
    let all: Vec<&Category> = sheet.categories.iter().collect();
    let two = sheet.categories.len() > 1 && command_col(&all, col2 - 4) + 2 + MIN_DESC + 4 <= col2;
    let (ncols, col_w) = if two { (2, col2) } else { (1, width) };

    // masonry: each panel goes to the currently shorter column
    let mut cols: Vec<Vec<(usize, Vec<Line>)>> = vec![Vec::new(); ncols];
    let mut heights = vec![0usize; ncols];
    for (i, cat) in sheet.categories.iter().enumerate() {
        let p = panel(cat, col_w, prim, 0);
        let c = (0..ncols).min_by_key(|&c| heights[c]).unwrap();
        heights[c] += p.len();
        cols[c].push((i, p));
    }
    // stretch the last panel of shorter columns so the bottoms line up
    if ncols == 2 {
        let max = *heights.iter().max().unwrap();
        for c in 0..ncols {
            let diff = max - heights[c];
            if diff > 0 {
                if let Some((i, p)) = cols[c].last_mut() {
                    *p = panel(&sheet.categories[*i], col_w, prim, diff);
                }
            }
        }
    }
    let flat: Vec<Vec<Line>> = cols.into_iter().map(|c| c.into_iter().flat_map(|(_, p)| p).collect()).collect();
    let rows = flat.iter().map(|c| c.len()).max().unwrap_or(0);
    for r in 0..rows {
        let mut line: Line = Vec::new();
        for (ci, col) in flat.iter().enumerate() {
            if ci > 0 {
                line.extend(spaces(1));
            }
            line.extend(markup::pad_right(col.get(r).cloned().unwrap_or_default(), col_w));
        }
        out.push(line);
    }
    out
}

/// One-line random tip: `sheet  command  — description`.
pub fn tip(sheet_name: &str, sheet: &Sheet, cat: &Category, e: &crate::sheet::Entry) -> Line {
    let prim = primary(sheet);
    let dim = Style { dim: Some(true), ..Default::default() };
    let mut l: Line = plain("💡 ", Style::default());
    l.extend(plain(&format!("{sheet_name} · {}: ", strip(&cat.name)), dim));
    l.extend(parse(&e.command, Style { bold: Some(true), ..prim }));
    l.extend(plain(" — ", dim));
    l.extend(parse(&e.description, Style::default()));
    l
}

fn strip(s: &str) -> String {
    parse(s, Style::default()).iter().map(|c: &Cell| c.ch).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet;

    #[test]
    fn tags_span_logo_lines() {
        let mut s = sheet::parse(r#"{"LOGO":[{"ascii":["[red]ab","c[/red]d"]}],"X":[]}"#).unwrap();
        s.title.clear();
        let lines = header(&s, 80);
        let text: String = lines.iter().flatten().map(|c| c.ch).collect();
        assert!(!text.contains('['), "{text}");
        let c = lines.iter().flatten().find(|c| c.ch == 'c').unwrap();
        assert_eq!(c.style.fg, Some(Color::Idx(1)));
    }

    #[test]
    fn all_builtin_sheets_render_rectangular() {
        for (name, src) in sheet::discover(&[]) {
            let s = sheet::load(&src).unwrap();
            for w in [40, 80, 120, 150] {
                let lines = render(&s, &Opts { width: w, header: true });
                for l in &lines {
                    assert!(markup::width(l) <= w.max(30), "{name} @ {w}: line too wide");
                }
            }
        }
    }
}
