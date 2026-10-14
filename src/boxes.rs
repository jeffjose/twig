//! The boxes twig draws its reports in (`twig timings`, `twig status`,
//! `twig info`): rounded, with the frame muted and a title in the top edge.

use crate::template::Style;
use crate::util::visible_width;

/// Lines of a box around `body`, with `title` in the top edge and `foot`
/// (if any) in the bottom one. Lines may hold escapes; they are measured by
/// what they show.
pub fn draw(title: &str, body: &[String], foot: Option<&str>) -> Vec<String> {
    let dim = Style::of("dim");
    let head = format!("─ {title} ");
    let tail = foot.map(|f| format!("─ {f} ")).unwrap_or_default();
    let inner = body
        .iter()
        .map(|l| visible_width(l))
        .max()
        .unwrap_or(0)
        .max(visible_width(&head).saturating_sub(1))
        .max(visible_width(&tail).saturating_sub(1));
    let edge = |text: &str, l: char, r: char| {
        let fill = "─".repeat((inner + 2).saturating_sub(visible_width(text)));
        dim.paint(&format!("{l}{text}{fill}{r}"))
    };
    let mut out = vec![edge(&head, '╭', '╮')];
    for l in body {
        let pad = " ".repeat(inner - visible_width(l));
        out.push(format!("{} {l}\x1b[0m{pad} {}", dim.paint("│"), dim.paint("│")));
    }
    out.push(edge(&tail, '╰', '╯'));
    out
}

/// Two boxes side by side, the shorter one padded below, when both fit in
/// `width` columns; otherwise one above the other.
pub fn beside(left: &[String], right: &[String], width: usize) -> Vec<String> {
    let lw = left.iter().map(|l| visible_width(l)).max().unwrap_or(0);
    let rw = right.iter().map(|l| visible_width(l)).max().unwrap_or(0);
    if lw + 2 + rw > width {
        let mut out = left.to_vec();
        out.push(String::new());
        out.extend(right.iter().cloned());
        return out;
    }
    (0..left.len().max(right.len()))
        .map(|i| {
            let l = left.get(i).map(String::as_str).unwrap_or("");
            let r = right.get(i).map(String::as_str).unwrap_or("");
            format!("{l}{}  {r}", " ".repeat(lw - visible_width(l)))
        })
        .collect()
}

/// A labeled row, its items separated by dots and wrapped under the first
/// one to fit `width` columns.
pub fn row(label: &str, items: &[String], width: usize) -> Vec<String> {
    const INDENT: usize = 10;
    let dot = Style::of("dim").paint(" · ");
    let mut lines = Vec::new();
    let mut cur = format!("{label}{}", " ".repeat(INDENT.saturating_sub(visible_width(label))));
    let mut col = INDENT.max(visible_width(label));
    for (i, item) in items.iter().enumerate() {
        let w = visible_width(item);
        if i > 0 {
            if col + 3 + w > width {
                lines.push(std::mem::replace(&mut cur, " ".repeat(INDENT)));
                col = INDENT;
            } else {
                cur.push_str(&dot);
                col += 3;
            }
        }
        cur.push_str(item);
        col += w;
    }
    lines.push(cur);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_fit_their_widest_line() {
        let b = draw("t", &["ab".into(), "\x1b[1mabcd\x1b[0m".into()], Some("f"));
        let widths: Vec<usize> = b.iter().map(|l| visible_width(l)).collect();
        assert!(widths.iter().all(|&w| w == widths[0]), "{widths:?}");
        assert_eq!(widths[0], 8);
    }

    #[test]
    fn rows_wrap_under_the_first_item() {
        let items: Vec<String> = ["aaaa", "bbbb", "cccc"].iter().map(|s| s.to_string()).collect();
        let r = row("x", &items, 22);
        assert_eq!(r.len(), 2);
        assert!(r[1].starts_with(&" ".repeat(10)));
    }
}
