use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, PlaylistItem, Tab};

/// Render the Playlists tab: left panel (playlist list) + right panel (tracks).
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if area.width < 30 || area.height < 5 { return; }
    let accent = app.accent();
    let theme = app.theme.clone();
    let border_style = Style::default().fg(theme.border);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // ── Left panel ─────────────────────────────────────────────────────────────
    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if app.active_tab == Tab::Playlists && app.playlists_tab.focus_left { Style::default().fg(accent) } else { border_style })
        .title(" Playlists ").title_style(Style::default().fg(theme.foreground));
    let left_inner = left_block.inner(chunks[0]);
    f.render_widget(left_block, chunks[0]);

    let left_items: Vec<ListItem> = app.playlists_tab.items.iter().map(|item| {
        let (label, style) = match item {
            PlaylistItem::Header(h) => (h.clone(), Style::default().fg(Color::Rgb(160, 155, 150))),
            PlaylistItem::Saved { name, .. } => (format!(" ♪ {name}"), Style::default().fg(theme.foreground)),
            PlaylistItem::Favorites => {
                let count = app.playlists_tab.favorites_count;
                (if count == 0 { "\u{f02d1} Favorites".to_string() } else { format!("\u{f02d1} Favorites ({count})") }, Style::default().fg(Color::Red))
            }
            PlaylistItem::Random => {
                let count = app.playlists_tab.random_count;
                (format!(" ? Random {count}"), Style::default().fg(theme.foreground))
            }
        };
        ListItem::new(Line::from(Span::styled(label, style)))
    }).collect();

    let mut left_state = ListState::default().with_selected(Some(app.playlists_tab.selected));
    let left_list = List::new(left_items).highlight_style(
        Style::default().bg(accent).fg(Color::Black).add_modifier(Modifier::BOLD)
    ).highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(left_list, left_inner, &mut left_state);

    // ── Right panel ────────────────────────────────────────────────────────────
    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if app.active_tab == Tab::Playlists && !app.playlists_tab.focus_left { Style::default().fg(accent) } else { border_style })
        .title(" Tracks ").title_style(Style::default().fg(theme.foreground));
    let right_inner = right_block.inner(chunks[1]);
    f.render_widget(right_block, chunks[1]);

    let track_items: Vec<ListItem> = app.playlists_tab.tracks.iter().map(|song| {
        let heart = if song.starred.is_some() { "\u{f02d1} " } else { "  " };
        let line = format!(" {heart}{}  {}", song.title, song.artist.as_deref().unwrap_or(""));
        ListItem::new(Line::from(Span::styled(line, Style::default().fg(theme.foreground))))
    }).collect();

    let track_count = track_items.len();
    let mut track_state = ListState::default().with_selected(if track_count > 0 { Some(app.playlists_tab.selected_track) } else { None });
    let track_list = List::new(track_items).highlight_style(
        Style::default().bg(accent).fg(Color::Black).add_modifier(Modifier::BOLD)
    ).highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(track_list, right_inner, &mut track_state);
}
