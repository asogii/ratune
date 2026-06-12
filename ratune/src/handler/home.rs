//! Home tab handler functions.

use crate::action::Direction;
use crate::app::{App, HomeSection, Tab};

impl App {
    pub(crate) fn handle_navigate_home(&mut self, dir: Direction) {
        if self.home.active_section == HomeSection::RecentAlbums {
            if !self.config.home_recent_albums_show_art {
                if self.home.recent_albums.is_empty() { return; }
                let max_idx = self.home.recent_albums.len().saturating_sub(1);
                let visible_rows = self.home_recent_albums_inner.map(|r| r.height as usize).unwrap_or(8).max(1);
                match dir {
                    Direction::Up | Direction::Top => {
                        self.home.album_selected_index = self.home.album_selected_index.saturating_sub(1);
                        if self.home.album_selected_index < self.home.album_scroll_offset {
                            self.home.album_scroll_offset = self.home.album_selected_index;
                        }
                    }
                    Direction::Down | Direction::Bottom => {
                        self.home.album_selected_index = (self.home.album_selected_index + 1).min(max_idx);
                        let scroll_end = self.home.album_scroll_offset + visible_rows.saturating_sub(1);
                        if self.home.album_selected_index > scroll_end {
                            self.home.album_scroll_offset = self.home.album_scroll_offset.saturating_add(1);
                        }
                    }
                    Direction::PageUp => {
                        self.home.album_selected_index = self.home.album_selected_index.saturating_sub(visible_rows);
                        self.home.album_scroll_offset = self.home.album_scroll_offset.min(self.home.album_selected_index);
                    }
                    Direction::PageDown => {
                        self.home.album_selected_index = (self.home.album_selected_index + visible_rows).min(max_idx);
                        let scroll_end = self.home.album_scroll_offset + visible_rows.saturating_sub(1);
                        if self.home.album_selected_index > scroll_end {
                            self.home.album_scroll_offset = self.home.album_selected_index.saturating_sub(visible_rows.saturating_sub(1));
                        }
                    }
                }
                return;
            }
            let max_idx = self.home.recent_albums.len().saturating_sub(1);
            if self.home.recent_albums.is_empty() { return; }
            let per_row = self.home_recent_albums_inner.map(|inner| crate::ui::kitty_art::art_strip_layout(inner.width, inner.height).per_row).unwrap_or(1).max(1);
            match dir {
                Direction::Up | Direction::Top => {
                    if self.home.album_selected_index < per_row {
                        self.home.album_selected_index = 0;
                        return;
                    }
                    self.home.album_selected_index = self.home.album_selected_index.saturating_sub(per_row);
                    if self.home.album_selected_index < self.home.album_scroll_offset {
                        self.home.album_scroll_offset = self.home.album_selected_index;
                    }
                }
                Direction::Down | Direction::Bottom => {
                    self.home.album_selected_index = (self.home.album_selected_index + per_row).min(max_idx);
                    let visible_rows = self.home_recent_albums_inner.map(|r| {
                        let layout = crate::ui::kitty_art::art_strip_layout(r.width, r.height);
                        layout.grid_rows as usize * layout.per_row
                    }).unwrap_or(per_row * 2);
                    let scroll_end = self.home.album_scroll_offset + visible_rows.saturating_sub(1);
                    if self.home.album_selected_index > scroll_end {
                        self.home.album_scroll_offset = self.home.album_selected_index.saturating_sub(visible_rows.saturating_sub(per_row));
                    }
                }
                Direction::PageUp => {
                    let step = (self.home_recent_albums_inner.map(|r| r.height as usize).unwrap_or(8).max(1)).saturating_sub(1);
                    for _ in 0..step {
                        if self.home.album_selected_index < per_row { break; }
                        self.home.album_selected_index = self.home.album_selected_index.saturating_sub(per_row);
                    }
                    if self.home.album_selected_index < self.home.album_scroll_offset {
                        self.home.album_scroll_offset = self.home.album_selected_index;
                    }
                }
                Direction::PageDown => {
                    let step = (self.home_recent_albums_inner.map(|r| r.height as usize).unwrap_or(8).max(1)).saturating_sub(1);
                    for _ in 0..step {
                        self.home.album_selected_index = (self.home.album_selected_index + per_row).min(max_idx);
                    }
                }
            }
        } else {
            let section_len = match self.home.active_section {
                HomeSection::RecentTracks => self.home.recent_tracks.len(),
                HomeSection::TopArtists => self.home.top_artists.len(),
                HomeSection::Rediscover => self.home.rediscover.len(),
                _ => 0,
            };
            if section_len == 0 { return; }
            match dir {
                Direction::Up | Direction::Top => {
                    self.home.selected_index = self.home.selected_index.saturating_sub(1);
                }
                Direction::Down | Direction::Bottom => {
                    self.home.selected_index = (self.home.selected_index + 1).min(section_len - 1);
                }
                Direction::PageUp => {
                    self.home.selected_index = self.home.selected_index.saturating_sub(16);
                }
                Direction::PageDown => {
                    self.home.selected_index = (self.home.selected_index + 16).min(section_len - 1);
                }
            }
        }
    }

    pub(crate) fn handle_select_home(&mut self) {
        match self.home.active_section {
            HomeSection::RecentAlbums => {
                let idx = self.home.album_selected_index;
                if let Some(album) = self.home.recent_albums.get(idx) {
                    let artist_name = album.artist_name.clone();
                    if self.kitty_apc_overlay_active() {
                        let _ = crate::ui::kitty_art::clear_image(self.in_tmux);
                        let _ = crate::ui::kitty_art::clear_art_strip(self.in_tmux);
                    }
                    if self.ratatui_art_ready() && !self.ratatui_uses_kitty_apc() {
                        self.clear_ratatui_art_state();
                    }
                    self.pending_artist_select = Some(artist_name);
                    self.active_tab = Tab::Browser;
                    self.clear_browser_search();
                    self.apply_pending_artist_select();
                }
            }
            HomeSection::RecentTracks => {
                let idx = self.home.selected_index;
                if let Some(record) = self.home.recent_tracks.get(idx).cloned() {
                    let song_id = record.song_id.clone();
                    let song = ratune_subsonic::Song {
                        id: song_id,
                        title: record.track_name.clone(),
                        artist: Some(record.artist_name.clone()),
                        artist_id: Some(record.artist_id.clone()),
                        album: Some(record.album_name.clone()),
                        album_id: Some(record.album_id.clone()),
                        duration: Some(record.duration_secs as u32),
                        track: None, disc_number: None, year: None, genre: None,
                        cover_art: None, path: None, suffix: None,
                        content_type: None, bit_rate: None, size: None, starred: None,
                    };
                    let was_empty = self.queue.songs.is_empty();
                    self.queue.push(song);
                    if was_empty { self.queue.cursor = 0; }
                    else { self.queue.cursor = self.queue.songs.len() - 1; }
                    self.play_gen += 1;
                    self.play_current();
                }
            }
            HomeSection::TopArtists => {
                if self.kitty_apc_overlay_active() {
                    let _ = crate::ui::kitty_art::clear_image(self.in_tmux);
                    let _ = crate::ui::kitty_art::clear_art_strip(self.in_tmux);
                }
                if self.ratatui_art_ready() && !self.ratatui_uses_kitty_apc() {
                    self.clear_ratatui_art_state();
                }
                self.active_tab = Tab::Browser;
                self.clear_browser_search();
            }
            HomeSection::Rediscover => {
                if let Some((_, artist_name)) = self.home.rediscover.get(self.home.selected_index) {
                    self.pending_artist_select = Some(artist_name.clone());
                }
                if self.kitty_apc_overlay_active() {
                    let _ = crate::ui::kitty_art::clear_image(self.in_tmux);
                    let _ = crate::ui::kitty_art::clear_art_strip(self.in_tmux);
                }
                if self.ratatui_art_ready() && !self.ratatui_uses_kitty_apc() {
                    self.clear_ratatui_art_state();
                }
                self.active_tab = Tab::Browser;
                self.apply_pending_artist_select();
                self.clear_browser_search();
            }
        }
    }
}
