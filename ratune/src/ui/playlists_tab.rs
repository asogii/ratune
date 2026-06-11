use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, PlaylistItem, Tab};

/// Render the Playlists tab: left panel (playlist list) + right panel (tracks).
pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    if area.width < 30 || area.height < 5 {
        return;
    }

    let accent = app.accent();
    let theme = app.theme.clone();
    let border_style = Style::default().fg(theme.border);

    // Split horizontally: left 35%, right 65%.
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    // ── Left panel: playlist list ──────────────────────────────────────────────
    let left_block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            if app.active_tab == Tab::Playlists && app.playlists_tab.focus_left {
                Style::default().fg(accent)
            } else {
                border_style
            },
        )
        .title(" Playlists ")
        .title_style(Style::default().fg(theme.foreground));
    let left_inner = left_block.inner(chunks[0]);
    f.render_widget(left_block, chunks[0]);

    let left_items: Vec<ListItem> = app
        .playlists_tab
        .items
        .iter()
        .map(|item| {
            let (label, style) = match item {
                PlaylistItem::Header(h) => {
                    (h.clone(), Style::default().fg(theme.dimmed))
                }
                PlaylistItem::Saved { name, .. } => {
                    (format!(" ♪ {name}"), Style::default().fg(theme.foreground))
                }
                PlaylistItem::Favorites => {
                    let count = app.playlists_tab.favorites_count;
                    let label = if count == 0 {
                        " ♥ Favorites".to_string()
                    } else {
                        format!(" ♥ Favorites ({count})")
                    };
                    (label, Style::default().fg(Color::Red))
                }
                PlaylistItem::Random => {
                    let count = app.playlists_tab.random_count;
                    (format!(" ? Random {count}"), Style::default().fg(theme.foreground))
                }
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let mut left_state = ListState::default().with_selected(Some(app.playlists_tab.selected));
    let left_list = List::new(left_items)
        .highlight_style(
            if app.playlists_tab.focus_left {
                Style::default()
                    .bg(accent)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.foreground)
            },
        )
        .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(left_list, left_inner, &mut left_state);

    // ── Right panel: tracks ────────────────────────────────────────────────────
    let right_block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            if app.active_tab == Tab::Playlists && !app.playlists_tab.focus_left {
                Style::default().fg(accent)
            } else {
                border_style
            },
        )
        .title(" Tracks ")
        .title_style(Style::default().fg(theme.foreground));
    let right_inner = right_block.inner(chunks[1]);
    f.render_widget(right_block, chunks[1]);

    let track_items: Vec<ListItem> = app
        .playlists_tab
        .tracks
        .iter()
        .map(|song| {
            let title = song.title.as_str();
            let artist = song.artist.as_deref().unwrap_or("");
            let line = format!(" {title}  {artist}");
            ListItem::new(Line::from(Span::styled(line, Style::default().fg(theme.foreground))))
        })
        .collect();

    let track_count = track_items.len();
    let mut track_state = ListState::default()
        .with_selected(if track_count > 0 { Some(app.playlists_tab.selected_track) } else { None });
    let track_list = List::new(track_items)
        .highlight_style(
            if app.playlists_tab.focus_left {
                // When left panel is focused, right panel highlight = no highlight
                Style::default().fg(theme.foreground)
            } else {
                Style::default()
                    .bg(accent)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            },
        )
        .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(track_list, right_inner, &mut track_state);

    // ── Save input overlay ─────────────────────────────────────────────────────
    if let Some(input) = &app.playlists_tab.save_input {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(theme.surface).fg(theme.foreground))
            .title(" Save queue as ");
        let input_area = Rect {
            x: area.x + area.width / 4,
            y: area.y + area.height / 2 - 1,
            width: area.width / 2,
            height: 3,
        };
        let input_para = Paragraph::new(Line::from(Span::styled(
            format!(" {input}█"),
            Style::default().fg(theme.foreground),
        )))
        .block(input_block);
        f.render_widget(input_para, input_area);
    }
}
