//! File diff view widget (full-screen)

use std::sync::LazyLock;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use similar::{ChangeTag, TextDiff};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::git::{DiffLineContent, DiffLineOrigin, FileChangeKind, FileDiffContent};

use super::{render_placeholder_block, MIN_WIDGET_HEIGHT, MIN_WIDGET_WIDTH};

// Syntax highlighting resources (initialized once)
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

const THEME_NAME: &str = "base16-eighties.dark";

// Diff background colors
const BG_ADD: Color = Color::Rgb(0, 50, 0);
const BG_DEL: Color = Color::Rgb(65, 0, 0);
const BG_ADD_EMPH: Color = Color::Rgb(0, 90, 0);
const BG_DEL_EMPH: Color = Color::Rgb(110, 0, 0);

// --- Line grouping for word-level emphasis ---

enum LineGroup<'a> {
    Context(&'a DiffLineContent),
    Change {
        deletions: Vec<&'a DiffLineContent>,
        additions: Vec<&'a DiffLineContent>,
    },
    NoNewline,
}

fn group_diff_lines<'a>(lines: &'a [DiffLineContent]) -> Vec<LineGroup<'a>> {
    let mut groups: Vec<LineGroup<'a>> = Vec::new();
    let mut pending_dels: Vec<&'a DiffLineContent> = Vec::new();
    let mut pending_adds: Vec<&'a DiffLineContent> = Vec::new();

    let flush = |groups: &mut Vec<LineGroup<'a>>,
                 dels: &mut Vec<&'a DiffLineContent>,
                 adds: &mut Vec<&'a DiffLineContent>| {
        if !dels.is_empty() || !adds.is_empty() {
            groups.push(LineGroup::Change {
                deletions: std::mem::take(dels),
                additions: std::mem::take(adds),
            });
        }
    };

    for line in lines {
        match line.origin {
            DiffLineOrigin::Context | DiffLineOrigin::HunkHeader => {
                flush(&mut groups, &mut pending_dels, &mut pending_adds);
                groups.push(LineGroup::Context(line));
            }
            DiffLineOrigin::Deletion => {
                if !pending_adds.is_empty() {
                    flush(&mut groups, &mut pending_dels, &mut pending_adds);
                }
                pending_dels.push(line);
            }
            DiffLineOrigin::Addition => {
                pending_adds.push(line);
            }
            DiffLineOrigin::NoNewlineAtEof => {
                flush(&mut groups, &mut pending_dels, &mut pending_adds);
                groups.push(LineGroup::NoNewline);
            }
        }
    }
    flush(&mut groups, &mut pending_dels, &mut pending_adds);
    groups
}

// --- Word-level emphasis via `similar` ---

struct WordEmphasis {
    old_spans: Vec<Vec<(bool, String)>>,
    new_spans: Vec<Vec<(bool, String)>>,
}

fn compute_word_emphasis(
    deletions: &[&DiffLineContent],
    additions: &[&DiffLineContent],
) -> WordEmphasis {
    let mut old_spans = Vec::with_capacity(deletions.len());
    let mut new_spans = Vec::with_capacity(additions.len());

    let max_len = deletions.len().max(additions.len());

    for i in 0..max_len {
        match (deletions.get(i), additions.get(i)) {
            (Some(del), Some(add)) => {
                let diff = TextDiff::from_words(&del.content, &add.content);
                let mut old_s = Vec::new();
                let mut new_s = Vec::new();
                for change in diff.iter_all_changes() {
                    match change.tag() {
                        ChangeTag::Equal => {
                            let v = change.value().to_string();
                            old_s.push((false, v.clone()));
                            new_s.push((false, v));
                        }
                        ChangeTag::Delete => {
                            old_s.push((true, change.value().to_string()));
                        }
                        ChangeTag::Insert => {
                            new_s.push((true, change.value().to_string()));
                        }
                    }
                }
                old_spans.push(old_s);
                new_spans.push(new_s);
            }
            (Some(del), None) => {
                old_spans.push(vec![(false, del.content.clone())]);
            }
            (None, Some(add)) => {
                new_spans.push(vec![(false, add.content.clone())]);
            }
            (None, None) => {}
        }
    }

    WordEmphasis {
        old_spans,
        new_spans,
    }
}

// --- Syntax highlighting helpers ---

fn highlight_line_owned(hl: &mut HighlightLines, content: &str) -> Vec<(SyntectStyle, String)> {
    let line = format!("{}\n", content);
    match hl.highlight_line(&line, &SYNTAX_SET) {
        Ok(spans) => {
            let result: Vec<_> = spans
                .into_iter()
                .map(|(style, text)| (style, text.trim_end_matches('\n').to_string()))
                .filter(|(_, text)| !text.is_empty())
                .collect();
            // Keep at least one span so diff background color is applied to blank lines
            if result.is_empty() {
                vec![(SyntectStyle::default(), String::new())]
            } else {
                result
            }
        }
        Err(_) => vec![(SyntectStyle::default(), content.to_string())],
    }
}

/// Map syntect RGB to vivid terminal ANSI colors (like delta/bat's ansi theme).
/// This ensures colors are always vivid regardless of the syntect theme,
/// because the terminal's own color scheme defines the actual appearance.
fn syntect_fg(style: &SyntectStyle) -> Color {
    let (r, g, b) = (style.foreground.r, style.foreground.g, style.foreground.b);

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let sat = if max > 0 {
        (max - min) as f32 / max as f32
    } else {
        0.0
    };

    // Achromatic (grays / whites)
    if sat < 0.15 {
        return if max < 90 {
            Color::DarkGray
        } else {
            Color::White
        };
    }

    // Compute hue (0–360)
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    let range = (max - min) as f32;
    let hue = if r == max {
        60.0 * ((gf - bf) / range).rem_euclid(6.0)
    } else if g == max {
        60.0 * ((bf - rf) / range + 2.0)
    } else {
        60.0 * ((rf - gf) / range + 4.0)
    };

    match hue as u16 {
        0..=20 | 346..=360 => Color::LightRed,
        21..=65 => Color::LightYellow,
        66..=155 => Color::LightGreen,
        156..=195 => Color::LightCyan,
        196..=265 => Color::LightBlue,
        _ => Color::LightMagenta,
    }
}

/// Convert syntax-highlighted spans to ratatui spans with optional background
fn syntax_to_ratatui(
    syn_spans: &[(SyntectStyle, String)],
    bg: Option<Color>,
) -> Vec<Span<'static>> {
    syn_spans
        .iter()
        .map(|(style, text)| {
            let mut s = Style::default().fg(syntect_fg(style));
            if let Some(bg) = bg {
                s = s.bg(bg);
            }
            Span::styled(text.clone(), s)
        })
        .collect()
}

/// Merge syntax spans with word-level emphasis spans
fn merge_syntax_and_emphasis(
    syn_spans: &[(SyntectStyle, String)],
    emp_spans: &[(bool, String)],
    base_bg: Color,
    emph_bg: Color,
) -> Vec<Span<'static>> {
    let mut result = Vec::new();

    let mut syn_idx = 0;
    let mut syn_off = 0usize;
    let mut emp_idx = 0;
    let mut emp_off = 0usize;

    loop {
        let syn = syn_spans.get(syn_idx);
        let emp = emp_spans.get(emp_idx);

        let (Some((syn_style, syn_text)), Some((emphasized, emp_text))) = (syn, emp) else {
            break;
        };

        let syn_rem = syn_text.len() - syn_off;
        let emp_rem = emp_text.len() - emp_off;

        if syn_rem == 0 {
            syn_idx += 1;
            syn_off = 0;
            continue;
        }
        if emp_rem == 0 {
            emp_idx += 1;
            emp_off = 0;
            continue;
        }

        let chunk_len = syn_rem.min(emp_rem);
        // Ensure we don't slice in the middle of a multi-byte UTF-8 character.
        // Both span lists cover the same text, but syntect and similar may
        // split at different token boundaries.
        let chunk_len = {
            let syn_remaining = &syn_text[syn_off..];
            let emp_remaining = &emp_text[emp_off..];
            let mut len = chunk_len;
            while len > 0
                && (!syn_remaining.is_char_boundary(len) || !emp_remaining.is_char_boundary(len))
            {
                len -= 1;
            }
            len
        };
        if chunk_len == 0 {
            // Cannot find a common char boundary; skip both spans to avoid infinite loop
            syn_idx += 1;
            syn_off = 0;
            emp_idx += 1;
            emp_off = 0;
            continue;
        }
        let text = &syn_text[syn_off..syn_off + chunk_len];
        let fg = syntect_fg(syn_style);
        let bg = if *emphasized { emph_bg } else { base_bg };

        result.push(Span::styled(
            text.to_string(),
            Style::default().fg(fg).bg(bg),
        ));

        syn_off += chunk_len;
        emp_off += chunk_len;

        if syn_off >= syn_text.len() {
            syn_idx += 1;
            syn_off = 0;
        }
        if emp_off >= emp_text.len() {
            emp_idx += 1;
            emp_off = 0;
        }
    }

    // Remaining syntax spans (no emphasis data)
    while let Some((syn_style, syn_text)) = syn_spans.get(syn_idx) {
        let text = &syn_text[syn_off..];
        if !text.is_empty() {
            result.push(Span::styled(
                text.to_string(),
                Style::default().fg(syntect_fg(syn_style)).bg(base_bg),
            ));
        }
        syn_idx += 1;
        syn_off = 0;
    }

    result
}

// --- Line rendering helpers ---

fn make_diff_line(dl: &DiffLineContent, content_spans: Vec<Span<'static>>) -> Line<'static> {
    let lineno_style = Style::default().fg(Color::DarkGray);

    let old_no = dl
        .old_lineno
        .map(|n| format!("{:>4}", n))
        .unwrap_or_else(|| "    ".to_string());
    let new_no = dl
        .new_lineno
        .map(|n| format!("{:>4}", n))
        .unwrap_or_else(|| "    ".to_string());

    let prefix = match dl.origin {
        DiffLineOrigin::Addition => "+",
        DiffLineOrigin::Deletion => "-",
        _ => " ",
    };
    let prefix_style = match dl.origin {
        DiffLineOrigin::Addition => Style::default().fg(Color::Green).bg(BG_ADD),
        DiffLineOrigin::Deletion => Style::default().fg(Color::Red).bg(BG_DEL),
        _ => Style::default(),
    };

    let mut spans = vec![
        Span::styled(old_no, lineno_style),
        Span::styled(" ", lineno_style),
        Span::styled(new_no, lineno_style),
        Span::raw(" "),
        Span::styled(prefix.to_string(), prefix_style),
    ];
    spans.extend(content_spans);

    Line::from(spans)
}

fn determine_syntax(path: &std::path::Path) -> &'static SyntaxReference {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Try full filename first (handles Dockerfile, Makefile, etc.), then extension
    SYNTAX_SET
        .find_syntax_by_extension(file_name)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(ext))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
}

// --- Public: pre-compute highlighted lines (called once on mode entry) ---

/// Build pre-computed highlighted lines and hunk header positions.
/// Returns `(rendered_lines, hunk_positions)` so that hunk navigation
/// positions are always in sync with the actual rendered output.
pub fn build_highlighted_lines(content: &FileDiffContent) -> (Vec<Line<'static>>, Vec<usize>) {
    if content.is_binary {
        return (
            vec![Line::from(Span::styled(
                "(Binary file - no diff available)",
                Style::default().fg(Color::DarkGray),
            ))],
            Vec::new(),
        );
    }

    if content.hunks.is_empty() {
        return (
            vec![Line::from(Span::styled(
                "(No textual changes)",
                Style::default().fg(Color::DarkGray),
            ))],
            Vec::new(),
        );
    }

    let syntax = determine_syntax(&content.path);
    let theme = THEME_SET
        .themes
        .get(THEME_NAME)
        .or_else(|| THEME_SET.themes.values().next())
        .expect("syntect must have at least one built-in theme");
    let mut lines = Vec::new();
    let mut hunk_positions = Vec::new();

    // Maintain highlight state across hunks so multi-line constructs
    // (block comments, strings, etc.) that span hunk boundaries are
    // colored correctly.
    let mut old_hl = HighlightLines::new(syntax, theme);
    let mut new_hl = HighlightLines::new(syntax, theme);

    for hunk in &content.hunks {
        // Blank line before each hunk header for readability
        lines.push(Line::from(""));

        hunk_positions.push(lines.len());
        lines.push(Line::from(Span::styled(
            hunk.header.clone(),
            Style::default()
                .fg(Color::LightCyan)
                .bg(Color::Rgb(30, 40, 55))
                .add_modifier(Modifier::BOLD),
        )));

        let groups = group_diff_lines(&hunk.lines);

        for group in &groups {
            match group {
                LineGroup::Context(dl) => {
                    let syn = highlight_line_owned(&mut old_hl, &dl.content);
                    let _ = highlight_line_owned(&mut new_hl, &dl.content);
                    let spans = syntax_to_ratatui(&syn, None);
                    lines.push(make_diff_line(dl, spans));
                }
                LineGroup::Change {
                    deletions,
                    additions,
                } => {
                    let emp = compute_word_emphasis(deletions, additions);

                    for (i, dl) in deletions.iter().enumerate() {
                        let syn = highlight_line_owned(&mut old_hl, &dl.content);
                        let spans = if let Some(emp_spans) = emp.old_spans.get(i) {
                            merge_syntax_and_emphasis(&syn, emp_spans, BG_DEL, BG_DEL_EMPH)
                        } else {
                            syntax_to_ratatui(&syn, Some(BG_DEL))
                        };
                        lines.push(make_diff_line(dl, spans));
                    }

                    for (i, dl) in additions.iter().enumerate() {
                        let syn = highlight_line_owned(&mut new_hl, &dl.content);
                        let spans = if let Some(emp_spans) = emp.new_spans.get(i) {
                            merge_syntax_and_emphasis(&syn, emp_spans, BG_ADD, BG_ADD_EMPH)
                        } else {
                            syntax_to_ratatui(&syn, Some(BG_ADD))
                        };
                        lines.push(make_diff_line(dl, spans));
                    }
                }
                LineGroup::NoNewline => {
                    lines.push(Line::from(Span::styled(
                        "\\ No newline at end of file",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }
    }

    (lines, hunk_positions)
}

// --- Widget (renders pre-computed lines) ---

pub struct FileDiffViewWidget<'a> {
    content: &'a FileDiffContent,
    rendered_lines: &'a [Line<'static>],
    scroll_offset: usize,
    file_position: String,
}

impl<'a> FileDiffViewWidget<'a> {
    pub fn new(
        content: &'a FileDiffContent,
        rendered_lines: &'a [Line<'static>],
        scroll_offset: usize,
        file_index: usize,
        file_count: usize,
    ) -> Self {
        Self {
            content,
            rendered_lines,
            scroll_offset,
            file_position: format!("[{}/{}]", file_index + 1, file_count),
        }
    }
}

impl<'a> Widget for FileDiffViewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < MIN_WIDGET_WIDTH || area.height < MIN_WIDGET_HEIGHT {
            render_placeholder_block(area, buf);
            return;
        }

        let indicator = match self.content.kind {
            FileChangeKind::Added => "A",
            FileChangeKind::Modified => "M",
            FileChangeKind::Deleted => "D",
            FileChangeKind::Renamed => "R",
            FileChangeKind::Copied => "C",
        };

        let path_str = self.content.path.to_string_lossy();
        let stats = if self.content.is_binary {
            "(binary)".to_string()
        } else {
            format!(
                "+{} -{}",
                self.content.total_additions, self.content.total_deletions
            )
        };

        let title = format!(
            " [{}] {}  {}  {} ",
            indicator, path_str, stats, self.file_position
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );

        // Pre-slice lines to avoid u16 overflow in Paragraph::scroll()
        // for diffs longer than 65535 lines.
        let visible_height = area.height.saturating_sub(2) as usize; // minus block borders
        let start = self.scroll_offset.min(
            self.rendered_lines.len().saturating_sub(1),
        );
        let end = (start + visible_height + 1).min(self.rendered_lines.len());
        let visible_lines = &self.rendered_lines[start..end];

        let paragraph = Paragraph::new(visible_lines.to_vec()).block(block);

        Widget::render(paragraph, area, buf);
    }
}
