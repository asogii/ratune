//! Playlists tab handler functions.
//!
//! Called from the main dispatch in `app.rs` via `self.handle_playlists_*()`.

use crate::app::{App, LibraryUpdate, PlaylistItem, Tab};

impl App {
    pub(crate) fn handle_playlists_append(&mut self) {
        if self.active_tab != Tab::Playlists || self.playlists_tab.focus_left {
            return;
        }
        if let Some(song) = self.playlists_tab.tracks.get(self.playlists_tab.selected_track).cloned() {
            let was_empty = self.queue.songs.is_empty();
            self.queue.push(song);
            if was_empty {
                self.queue.cursor = 0;
                self.play_current();
            }
        }
    }

    pub(crate) fn handle_playlists_prepend(&mut self) {
        if self.active_tab != Tab::Playlists || self.playlists_tab.focus_left {
            return;
        }
        if let Some(song) = self.playlists_tab.tracks.get(self.playlists_tab.selected_track).cloned() {
            self.queue.prepend_songs(vec![song]);
        }
    }

    pub(crate) fn handle_playlists_insert_next(&mut self) {
        if self.active_tab != Tab::Playlists || self.playlists_tab.focus_left {
            return;
        }
        if let Some(song) = self.playlists_tab.tracks.get(self.playlists_tab.selected_track).cloned() {
            let insert_at = self.queue.cursor + 1;
            if insert_at <= self.queue.songs.len() {
                self.queue.songs.insert(insert_at, song);
            }
        }
    }

    pub(crate) fn handle_playlists_toggle_count(&mut self) {
        if self.active_tab != Tab::Playlists {
            return;
        }
        let idx = self.playlists_tab.selected;
        let Some(item) = self.playlists_tab.items.get(idx).cloned() else {
            return;
        };
        match item {
            PlaylistItem::Favorites => {
                let count = match self.playlists_tab.favorites_count {
                    0 => 20,
                    20 => 50,
                    50 => 100,
                    _ => 0,
                };
                self.playlists_tab.favorites_count = count;
                self.playlists_tab.load_gen = self.playlists_tab.load_gen.wrapping_add(1);
                let client = self.subsonic.clone();
                let tx = self.library_tx.clone();
                let gen = self.playlists_tab.load_gen;
                self.playlists_tab.tracks.clear();
                tokio::spawn(async move {
                    match client.get_starred().await {
                        Ok(mut starred) => {
                            if count > 0 {
                                starred.truncate(count as usize);
                            }
                            let _ = tx.send(LibraryUpdate::PlaylistsTabTracks(starred, gen)).await;
                        }
                        Err(e) => eprintln!("fetch favorites: {e}"),
                    }
                });
            }
            PlaylistItem::Random => {
                let count = match self.playlists_tab.random_count {
                    20 => 50,
                    50 => 100,
                    _ => 20,
                };
                self.playlists_tab.random_count = count;
                self.playlists_tab.random_tracks_cached = None;
                self.playlists_tab.load_gen = self.playlists_tab.load_gen.wrapping_add(1);
                self.refetch_random_playlist();
            }
            _ => {}
        }
    }

    pub(crate) fn handle_playlists_rerandom(&mut self) {
        if self.active_tab != Tab::Playlists {
            return;
        }
        let idx = self.playlists_tab.selected;
        if !matches!(self.playlists_tab.items.get(idx), Some(PlaylistItem::Random)) {
            return;
        }
        self.playlists_tab.random_tracks_cached = None;
        self.playlists_tab.load_gen = self.playlists_tab.load_gen.wrapping_add(1);
        self.refetch_random_playlist();
    }

    pub(crate) fn handle_playlists_delete_track(&mut self) {
        if self.active_tab != Tab::Playlists {
            return;
        }
        if self.playlists_tab.focus_left {
            let idx = self.playlists_tab.selected;
            if let Some(PlaylistItem::Saved { name, .. }) = self.playlists_tab.items.get(idx).cloned() {
                self.playlists_tab.pending_delete_playlist = Some(name.clone());
            }
            return;
        }
        let c = self.playlists_tab.selected_track;
        if c >= self.playlists_tab.tracks.len() {
            return;
        }
        let idx = self.playlists_tab.selected;
        if matches!(self.playlists_tab.items.get(idx), Some(PlaylistItem::Favorites)) {
            if let Some(song) = self.playlists_tab.tracks.get(c) {
                let song_id = song.id.clone();
                let client = self.subsonic.clone();
                tokio::spawn(async move {
                    let _ = client.unstar_song(&song_id).await;
                });
            }
            self.playlists_tab.tracks.remove(c);
            self.playlists_tab.tracks_cache.insert(idx, self.playlists_tab.tracks.clone());
            self.playlists_tab.selected_track =
                self.playlists_tab.selected_track.min(self.playlists_tab.tracks.len().saturating_sub(1));
            return;
        }
        self.playlists_tab.tracks.remove(c);
        self.playlists_tab.tracks_cache.insert(idx, self.playlists_tab.tracks.clone());
        self.playlists_tab.selected_track =
            self.playlists_tab.selected_track.min(self.playlists_tab.tracks.len().saturating_sub(1));
        if self.playlists_tab.dirty_since.is_none() {
            self.playlists_tab.dirty_since = Some(std::time::Instant::now());
        }
    }

    pub(crate) fn handle_playlists_save_queue(&mut self) {
        if self.active_tab == Tab::Playlists || self.active_tab == Tab::NowPlaying {
            if self.playlists_tab.save_input.is_some() {
                self.finish_save_queue();
            } else {
                self.playlists_tab.save_input = Some(String::new());
            }
        }
    }

    pub(crate) fn handle_playlists_confirm_delete(&mut self) {
        if let Some(name) = self.playlists_tab.pending_delete_playlist.take() {
            if let Ok(dir) = crate::persist::playlists_dir() {
                let path = dir.join(format!("{name}.json"));
                let _ = std::fs::remove_file(&path);
            }
            self.refresh_playlists_tab();
        }
    }
}
