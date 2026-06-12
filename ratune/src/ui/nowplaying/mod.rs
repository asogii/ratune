use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use ratatui_image::thread::ThreadProtocol;
use ratatui_image::StatefulImage;

use crate::app::App;
use crate::theme::style_with_bg;
use crate::ui::{layout, now_playing, queue, visualizer};

pub fn render(app: &mut App, frame: &mut Frame, area: Rect) {
    let boxed = app
        .config
        .now_playing_layout
        .trim()
        .eq_ignore_ascii_case("boxed");
    let show_art = app.config.nowplaying_show_art;
    let art_position = layout::placement_from_str(&app.config.nowplaying_art_position)
        .unwrap_or(layout::Placement::Left);
    let queue_position = layout::placement_from_str(&app.config.nowplaying_queue_position)
        .unwrap_or(layout::Placement::Right);
    let visualizer_position = layout::placement_from_str(&app.config.visualizer_location)
        .unwrap_or(layout::Placement::Right);
    let now_playing_position =
        layout::placement_from_str(&app.config.now_playing_box_location)
            .unwrap_or(layout::Placement::Right);
    let lyrics_position =
        layout::placement_from_str(&app.config.lyrics_location).unwrap_or(queue_position);

    let rects = layout::now_playing_rects(
        area,
        show_art,
        art_position,
        queue_position,
        app.config.nowplaying_left_width_percent,
        app.config.nowplaying_vertical_fill_top_percent,
        app.visualizer_visible,
        visualizer_position,
        app.lyrics_visible,
        lyrics_position,
        boxed,
        now_playing_position,
    );

    let Some(npfmt) = &app.playback.current_song else {
        let body = Paragraph::new(Line::from(Span::styled(
            "No track loaded",
            Style::default().fg(app.theme.dimmed),
        )));
        if boxed {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(app.theme.border))
                .style(style_with_bg(app.theme.surface));
            frame.render_widget(body.block(block), area);
        } else {
            frame.render_widget(body, area);
        }
        return;
    };

    // ── NP info with album art picker ──────────────────────────────────────────
    let np_area = rects.art;
    if show_art && np_area.is_some() {
        if let Some(picker) = app.art_picker.as_mut() {
            picker.set_background_color(crate::theme::color_to_rgba(app.theme.surface));
        }

        if let Some(picker) = app.art_picker.as_ref() {
            if let Some(cover_id) = &npfmt.cover_art {
                if let Some(bytes) = app.art_cache.as_ref().and_then(|(id, b)| (id == cover_id).then_some(b)) {
                    let inner = np_area.unwrap();
                    if inner.width == 0 || inner.height == 0 {
                        return;
                    }
                    let fs = picker.font_size();
                    let thumb_rect = Rect {
                        x: inner.x,
                        y: inner.y,
                        width: inner.width,
                        height: inner.height,
                    };
                    let Ok(img) = image::load_from_memory(bytes) else {
                        return;
                    };
                    let art_rect =
                        crate::ui::art_prepare::contain_fit_rect_in_cells(&img, thumb_rect, fs);

                    if art_rect.width == 0 || art_rect.height == 0 {
                        return;
                    }

                    let prep = crate::ui::art_prepare::prepare_art_image_for_rect_contain_fit(
                        img, art_rect, fs,
                    );
                    let proto = picker.new_resize_protocol(prep);
                    let img_resize = app.ratatui_stateful_resize();

                    let w = StatefulImage::default().resize(img_resize);
                    let mut state = proto;
                    frame.render_stateful_widget(w, art_rect, &mut state);
                }
            }
        }
    }

    // ── Queue ─────────────────────────────────────────────────────────────────
    let queue_area = rects.queue;
    if queue_area.width > 0 && queue_area.height > 0 {
        let is_active = !app.lyrics_visible && !app.visualizer_visible;
        queue::render(app, frame, queue_area, is_active);
    }

    // ── Visualizer ────────────────────────────────────────────────────────────
    if app.visualizer_visible {
        if let Some(vis_area) = rects.visualizer {
            if vis_area.width > 0 && vis_area.height > 0 {
                visualizer::render_visualizer_ex(app, frame, vis_area);
            }
        }
    }

    // ── Lyrics ────────────────────────────────────────────────────────────────
    if app.lyrics_visible {
        if let Some(lyrics_area) = rects.lyrics {
            if lyrics_area.width > 0 && lyrics_area.height > 0 {
                if app.lyrics_are_unsynced() {
                    crate::ui::now_playing_format::render_unsynced_lyrics_block(
                        app, frame, lyrics_area,
                    );
                } else if let Some((_, lines)) = &app.lyrics_cache {
                    crate::ui::now_playing_format::render_synced_lyrics_progress(
                        app, frame, lyrics_area, lines,
                    );
                }
            }
        }
    }
}
