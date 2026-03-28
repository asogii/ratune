use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;

// ── Top-level: 3-column Spotify-style bar ────────────────────────────────────

pub fn render(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // track info
            Constraint::Percentage(40), // transport controls
            Constraint::Percentage(30), // progress + time
        ])
        .split(area);

    render_track_info(app, frame, cols[0]);
    render_controls(app, frame, cols[1]);
    render_progress(app, frame, cols[2]);
}

// ── Left 30%: track title (accent/bold) + artist (muted) + quality tag ───────

fn render_track_info(app: &App, frame: &mut Frame, area: Rect) {
    let t = &app.theme;
    let lines: Vec<Line> = if let Some(song) = &app.playback.current_song {
        let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
        // Quality tag: codec name for lossless, bitrate string otherwise.
        let quality = format_quality(song);
        let mut rows = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    song.title.as_str(),
                    Style::default().fg(app.accent()).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(artist, Style::default().fg(t.dimmed)),
            ]),
        ];
        if let Some(q) = quality {
            rows.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(q, Style::default().fg(t.dimmed)),
            ]));
        } else {
            rows.push(Line::from(""));
        }
        rows
    } else {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Not playing", Style::default().fg(t.dimmed)),
            ]),
            Line::from(""),
            Line::from(""),
        ]
    };
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(t.surface)),
        area,
    );
}

/// Format the audio quality label for the now-playing bar.
/// Returns `None` when no quality info is available.
fn format_quality(song: &playterm_subsonic::Song) -> Option<String> {
    let lossless = song.suffix.as_deref()
        .map(|s| matches!(s.to_lowercase().as_str(), "flac" | "wav" | "alac" | "ape" | "aiff"))
        .unwrap_or(false);

    if lossless {
        let fmt = song.suffix.as_deref().unwrap_or("").to_uppercase();
        return Some(fmt);
    }
    song.bit_rate.map(|br| format!("{}kbps", br))
}

// ── Center 40%: transport controls, centered ─────────────────────────────────

fn render_controls(app: &App, frame: &mut Frame, area: Rect) {
    let t = &app.theme;
    let (play_label, play_style) = if app.playback.current_song.is_none() {
        ("▶", Style::default().fg(t.dimmed))
    } else if app.playback.paused {
        ("( ▶ )", Style::default().fg(app.accent()).add_modifier(Modifier::BOLD))
    } else {
        ("( ⏸ )", Style::default().fg(app.accent()).add_modifier(Modifier::BOLD))
    };

    let sep = Style::default().fg(t.dimmed);
    let controls = Line::from(vec![
        Span::styled("  ⇄  ", Style::default().fg(TEXT_MUTED)),
        Span::styled("  ⏮  ", Style::default().fg(TEXT_MUTED)),
        Span::styled(play_label, play_style),
        Span::styled("  ⏭  ", Style::default().fg(TEXT_MUTED)),
        Span::styled("  ↻  ", Style::default().fg(TEXT_MUTED)),
    ]);

    // Vertically: 1 blank row, controls row, 2 blank rows → sits at row 1 of 4.
    let lines: Vec<Line> = vec![
        Line::from(""),
        controls,
        Line::from(""),
        Line::from(""),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().bg(t.surface)),
        area,
    );
}

// ── Right 30%: progress gauge + elapsed / total ───────────────────────────────

fn render_progress(app: &App, frame: &mut Frame, area: Rect) {
    let t = &app.theme;
    let (elapsed_str, total_str, ratio) = if app.playback.current_song.is_some() {
        let e = app.playback.elapsed.as_secs();
        let elapsed_str = format!("{}:{:02}", e / 60, e % 60);
        let (total_str, ratio) = match app.playback.total {
            Some(tot) => {
                let ts = tot.as_secs();
                let r = if ts > 0 { (e as f64 / ts as f64).clamp(0.0, 1.0) } else { 0.0 };
                (format!("{}:{:02}", ts / 60, ts % 60), r)
            }
            None => ("--:--".to_string(), 0.0),
        };
        (elapsed_str, total_str, ratio)
    } else {
        ("0:00".to_string(), "0:00".to_string(), 0.0)
    };

    // Bar width: column width minus elapsed, total, and two 2-space gaps.
    let col_w = area.width as usize;
    let bar_w = col_w.saturating_sub(elapsed_str.len() + total_str.len() + 4);

    // Sub-character-cell bar using Unicode fractional blocks.
    // Each cell = 8 units; FRAC[i] fills (i+1)/8 of a cell.
    const FRAC: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    let units       = ((ratio * bar_w as f64 * 8.0) as usize).min(bar_w * 8);
    let full        = units / 8;
    let frac        = units % 8;
    let has_partial = frac > 0 && full < bar_w;
    let empty       = bar_w - full - usize::from(has_partial);

    let filled_str:  String = "█".repeat(full);
    let partial_str: String = if has_partial { FRAC[frac - 1].to_string() } else { String::new() };
    let empty_str:   String = "░".repeat(empty);

    let accent_color = app.accent();

    let progress = Line::from(vec![
        Span::styled(elapsed_str, Style::default().fg(t.dimmed)),
        Span::raw("  "),
        Span::styled(filled_str,  Style::default().fg(accent_color)),
        Span::styled(partial_str, Style::default().fg(accent_color)),
        Span::styled(empty_str,   Style::default().fg(t.dimmed)),
        Span::raw("  "),
        Span::styled(total_str, Style::default().fg(t.dimmed)),
    ]);

    // Row 0: empty, Row 1: empty, Row 2: progress, Row 3: empty.
    let lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(""),
        progress,
        Line::from(""),
    ];

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(t.surface)),
        area,
    );
}
