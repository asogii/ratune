//! Spectrum visualizer — braille dot renderer.
//!
//! Uses Unicode braille characters (U+2800–U+28FF) to render frequency bars
//! with 4× the vertical resolution of block characters.  Each terminal cell
//! holds a 2×4 dot grid; only the left column (dots 1-3 and 7) is used so
//! bars are single-column-wide.
//!
//! Braille bit layout for the left column (col 0), top → bottom:
//!   row 0 → bit 0 (value   1)
//!   row 1 → bit 1 (value   2)
//!   row 2 → bit 2 (value   4)
//!   row 3 → bit 6 (value  64)
//!
//! Filling from the bottom, `n` dots lit in one cell:
//!   n=1 → 0x40 → U+2840
//!   n=2 → 0x44 → U+2844
//!   n=3 → 0x46 → U+2846
//!   n=4 → 0x47 → U+2847   (fully filled left column)

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Braille codepoints for 0-4 left-column dots filled from the bottom.
/// Index = number of filled dots.
const LEFT_COL: [u32; 5] = [
    0x2800, // 0 dots — blank (skipped in rendering)
    0x2840, // 1 dot  — bit 6
    0x2844, // 2 dots — bits 2,6
    0x2846, // 3 dots — bits 1,2,6
    0x2847, // 4 dots — bits 0,1,2,6  (full left column)
];

/// Render a spectrum visualizer into `area` using braille characters.
///
/// - One bar per column: `num_bars = area.width` (all available columns).
/// - Vertical resolution is 4 dot-rows per cell row.
/// - Only the left dot column is lit, giving thin single-column bars.
/// - The last 2 of 32 computed bands are dropped (noisy high-frequency tail).
///
/// Does nothing if all bands are zero (startup / toggled off).
pub fn render_visualizer(f: &mut Frame, area: Rect, bands: &[f32], accent: Color) {
    if area.width == 0 || area.height == 0 || bands.is_empty() {
        return;
    }
    if bands.iter().all(|&b| b == 0.0) {
        return;
    }

    // Drop the last 2 bands (sparse single-bin high-frequency tail).
    let visible_bands = bands.len().saturating_sub(2);
    if visible_bands == 0 {
        return;
    }

    let num_bars = area.width as usize; // one bar per column
    let max_dots = area.height as usize * 4; // 4 dot-rows per cell row

    for i in 0..num_bars {
        // Map column index evenly across the 30 visible bands.
        let band_idx = i * visible_bands / num_bars;
        let band_val = bands[band_idx].clamp(0.0, 1.0);

        let total_dots = ((band_val * max_dots as f32) as usize).min(max_dots);
        let full_cells = (total_dots / 4).min(area.height as usize);
        let partial_dots = if full_cells < area.height as usize { total_dots % 4 } else { 0 };
        let has_partial = partial_dots > 0;
        let bar_rows = full_cells + usize::from(has_partial);

        if bar_rows == 0 {
            continue; // silent bar — leave column blank
        }

        let empty_rows = area.height as usize - bar_rows;

        // Build lines top → bottom for this single-column bar.
        let mut lines: Vec<Line> = Vec::with_capacity(area.height as usize);

        for _ in 0..empty_rows {
            lines.push(Line::from(" "));
        }
        if has_partial {
            let ch = char::from_u32(LEFT_COL[partial_dots]).unwrap_or(' ');
            lines.push(Line::from(Span::styled(
                ch.to_string(),
                Style::default().fg(accent),
            )));
        }
        for _ in 0..full_cells {
            // U+2847 — all four left-column dots filled
            lines.push(Line::from(Span::styled(
                "\u{2847}",
                Style::default().fg(accent),
            )));
        }

        f.render_widget(
            Paragraph::new(lines),
            Rect::new(area.x + i as u16, area.y, 1, area.height),
        );
    }
}
