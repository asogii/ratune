//! Queue and player handler functions.

use crate::app::{App, Tab};
use crate::state::RepeatMode;
use ratune_player::PlayerCommand;

impl App {
    pub(crate) fn handle_navigate_queue(&mut self, dir: crate::action::Direction) {
        let len = self.queue.songs.len();
        if len == 0 { return; }
        let page = self.queue_viewport_rows.max(1);
        use crate::action::Direction;
        self.queue.cursor = match dir {
            Direction::Up => self.queue.cursor.saturating_sub(1),
            Direction::Down => (self.queue.cursor + 1).min(len - 1),
            Direction::Top => 0,
            Direction::Bottom => len - 1,
            Direction::PageUp => self.queue.cursor.saturating_sub(page),
            Direction::PageDown => (self.queue.cursor + page).min(len - 1),
        };
    }

    pub(crate) fn handle_queue_move_up(&mut self) {
        if self.active_tab == Tab::Playlists && !self.playlists_tab.focus_left {
            let c = self.playlists_tab.selected_track;
            if c == 0 || self.playlists_tab.tracks.is_empty() { return; }
            self.playlists_tab.tracks.swap(c, c - 1);
            self.playlists_tab.selected_track = c - 1;
            self.playlists_tab.tracks_cache.insert(self.playlists_tab.selected, self.playlists_tab.tracks.clone());
            if c <= self.playlists_tab.track_scroll { self.playlists_tab.track_scroll = self.playlists_tab.track_scroll.saturating_sub(1); }
            return;
        }
        if self.active_tab != Tab::NowPlaying { return; }
        let c = self.queue.cursor;
        if c == 0 || self.queue.songs.is_empty() { return; }
        self.queue.songs.swap(c, c - 1);
        self.queue.cursor = c - 1;
        if c <= self.queue.scroll { self.queue.scroll = self.queue.scroll.saturating_sub(1); }
    }

    pub(crate) fn handle_queue_move_down(&mut self) {
        if self.active_tab == Tab::Playlists && !self.playlists_tab.focus_left {
            let c = self.playlists_tab.selected_track;
            if c + 1 >= self.playlists_tab.tracks.len() { return; }
            self.playlists_tab.tracks.swap(c, c + 1);
            self.playlists_tab.selected_track = c + 1;
            self.playlists_tab.tracks_cache.insert(self.playlists_tab.selected, self.playlists_tab.tracks.clone());
            if c + 1 > self.playlists_tab.track_scroll + 14 { self.playlists_tab.track_scroll = self.playlists_tab.track_scroll.saturating_add(1); }
            return;
        }
        if self.active_tab != Tab::NowPlaying { return; }
        let c = self.queue.cursor;
        if c + 1 >= self.queue.songs.len() { return; }
        self.queue.songs.swap(c, c + 1);
        self.queue.cursor = c + 1;
        let viewport_end = self.queue.scroll + self.queue_viewport_rows.max(1).saturating_sub(1);
        if c + 1 > viewport_end { self.queue.scroll = self.queue.scroll.saturating_add(1); }
    }

    pub(crate) fn handle_clear_queue(&mut self) {
        self.queue.songs.clear();
        self.queue.cursor = 0;
        self.queue.scroll = 0;
        self.queue.pre_shuffle_order = None;
        let _ = self.player_tx.send(PlayerCommand::Stop);
        self.playback.current_song = None;
        self.playback.elapsed = std::time::Duration::ZERO;
        self.playback.player_loaded = false;
    }

    pub(crate) fn handle_remove_from_queue(&mut self) {
        let idx = self.queue.cursor;
        let _ = self.queue.remove_at(idx);
        if self.queue.songs.is_empty() {
            self.queue.cursor = 0;
            self.queue.scroll = 0;
            let _ = self.player_tx.send(PlayerCommand::Stop);
            self.playback.current_song = None;
            self.playback.elapsed = std::time::Duration::ZERO;
            self.playback.player_loaded = false;
        } else {
            self.queue.cursor = self.queue.cursor.min(self.queue.songs.len().saturating_sub(1));
            if self.queue.cursor != idx { self.play_current(); }
            else { self.playback.elapsed = std::time::Duration::ZERO; self.playback.player_loaded = false; }
        }
    }

    pub(crate) fn handle_shuffle(&mut self) {
        use rand::seq::SliceRandom;
        if self.queue.songs.is_empty() { return; }
        let current = self.queue.current().cloned();
        let order: Vec<_> = self.queue.songs.iter().enumerate().filter(|(i, _)| Some(*i) != self.queue.songs.iter().position(|s| s.id == current.as_ref().map(|c| &c.id).cloned().unwrap_or_default())).map(|(_, s)| s.clone()).collect();
        let mut shuffled = order;
        shuffled.shuffle(&mut rand::thread_rng());
        if let Some(cur) = current {
            self.queue.songs = vec![cur];
            self.queue.songs.extend(shuffled);
        } else { self.queue.songs = shuffled; }
        self.queue.cursor = 0;
        self.queue.scroll = 0;
        self.queue.pre_shuffle_order = Some(self.queue.songs.clone());
    }

    pub(crate) fn handle_unshuffle(&mut self) {
        let Some(original) = self.queue.pre_shuffle_order.clone() else { return; };
        let current_id = self.queue.current().map(|s| s.id.clone());
        self.queue.songs = original;
        if let Some(id) = current_id {
            if let Some(idx) = self.queue.songs.iter().position(|s| s.id == id) {
                self.queue.cursor = idx;
                self.queue.scroll_clamp_cursor_visible(self.queue_viewport_rows.max(1));
            }
        }
    }

    pub(crate) fn handle_toggle_repeat_mode(&mut self) {
        self.playback.repeat_mode = match self.playback.repeat_mode {
            RepeatMode::None => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::None,
        };
    }

    pub(crate) fn handle_toggle_favorite(&mut self) {
        let Some(song) = self.playback.current_song.as_ref() else { return; };
        let song_id = song.id.clone();
        let is_starred = song.starred.is_some();
        let client = self.subsonic.clone();
        tokio::spawn(async move {
            if is_starred { let _ = client.unstar_song(&song_id).await; }
            else { let _ = client.star_song(&song_id).await; }
        });
        if let Some(s) = self.playback.current_song.as_mut() {
            if s.starred.is_some() { s.starred = None; } else { s.starred = Some(String::new()); }
        }
    }
}
