//! Browser tab handler functions.

use crate::app::{AddAllMode, App, BrowserColumn, Tab};
use crate::action::Direction;
use crate::state::LoadingState;
use ratune_player::PlayerCommand;
use std::time::Duration;
use std::time::Duration;

impl App {
    pub(crate) fn handle_focus_left(&mut self) {
        if self.active_tab == Tab::Browser {
            if self.browse_files() {
                match self.browser_focus {
                    BrowserColumn::Tracks => self.browser_focus = BrowserColumn::Albums,
                    BrowserColumn::Albums => { self.browser_focus = BrowserColumn::Artists; self.sync_folder_preview_from_left(); }
                    BrowserColumn::Artists => {}
                }
                return;
            }
            self.browser_focus = self.browser_focus.left();
            return;
        }
        if self.active_tab == Tab::Playlists { self.playlists_tab.focus_left = true; }
    }

    pub(crate) fn handle_focus_right(&mut self) {
        if self.active_tab == Tab::Browser {
            if self.browse_files() {
                match self.browser_focus {
                    BrowserColumn::Artists => { self.browser_focus = BrowserColumn::Tracks; self.sync_folder_preview_from_left(); }
                    BrowserColumn::Albums | BrowserColumn::Tracks => {}
                }
                return;
            }
            match self.browser_focus {
                BrowserColumn::Artists => {
                    if let Some(artist) = self.library.current_artist() {
                        let id = artist.id.clone();
                        if !self.library.albums.contains_key(&id) { self.library.albums.insert(id.clone(), LoadingState::Loading); self.fetch_albums(id); }
                    }
                    self.browser_focus = BrowserColumn::Albums;
                }
                BrowserColumn::Albums => {
                    if let Some(album) = self.library.current_album() {
                        let id = album.id.clone();
                        if !self.library.tracks.contains_key(&id) { self.library.tracks.insert(id.clone(), LoadingState::Loading); self.fetch_tracks(id); }
                    }
                    self.browser_focus = BrowserColumn::Tracks;
                }
                BrowserColumn::Tracks => {}
            }
            return;
        }
        if self.active_tab == Tab::Playlists && !self.playlists_tab.tracks.is_empty() {
            self.playlists_tab.focus_left = false;
        }
    }

    pub(crate) fn handle_navigate_browser(&mut self, dir: Direction, line_steps: usize) {
        if self.browse_files() { self.handle_navigate_browser_files(dir, line_steps); return; }
        match self.browser_focus {
            BrowserColumn::Artists => {
                if let LoadingState::Loaded(artists) = &self.library.artists {
                    let indices: Vec<usize> = if let Some(q) = self.browser_column_filter(BrowserColumn::Artists) {
                        artists.iter().enumerate().filter(|(_, a)| a.name.to_lowercase().contains(q)).map(|(i, _)| i).collect()
                    } else { (0..artists.len()).collect() };
                    if indices.is_empty() { return; }
                    let cur = self.library.selected_artist.and_then(|sel| indices.iter().position(|&i| i == sel)).unwrap_or(0);
                    let page = self.browser_list_viewport_rows.max(1);
                    let new = match dir {
                        Direction::Up => cur.saturating_sub(line_steps), Direction::Down => (cur + line_steps).min(indices.len()-1),
                        Direction::Top => 0, Direction::Bottom => indices.len()-1,
                        Direction::PageUp => cur.saturating_sub(page), Direction::PageDown => (cur+page).min(indices.len()-1),
                    };
                    let orig = indices[new];
                    self.library.selected_artist = Some(orig);
                    self.library.selected_album = Some(0); self.library.selected_track = Some(0);
                    if !self.library.albums.contains_key(&artists[orig].id) {
                        self.library.albums.insert(artists[orig].id.clone(), LoadingState::Loading);
                        self.fetch_albums(artists[orig].id.clone());
                    }
                }
            }
            BrowserColumn::Albums => {
                let Some(artist_id) = self.library.current_artist().map(|a| a.id.clone()) else { return; };
                if let Some(LoadingState::Loaded(albums)) = self.library.albums.get(&artist_id) {
                    let indices: Vec<usize> = if let Some(q) = self.browser_column_filter(BrowserColumn::Albums) {
                        albums.iter().enumerate().filter(|(_, a)| a.name.to_lowercase().contains(q)).map(|(i, _)| i).collect()
                    } else { (0..albums.len()).collect() };
                    if indices.is_empty() { return; }
                    let cur = self.library.selected_album.and_then(|sel| indices.iter().position(|&i| i == sel)).unwrap_or(0);
                    let page = self.browser_list_viewport_rows.max(1);
                    let new = match dir {
                        Direction::Up => cur.saturating_sub(line_steps), Direction::Down => (cur+line_steps).min(indices.len()-1),
                        Direction::Top => 0, Direction::Bottom => indices.len()-1,
                        Direction::PageUp => cur.saturating_sub(page), Direction::PageDown => (cur+page).min(indices.len()-1),
                    };
                    let orig = indices[new];
                    self.library.selected_album = Some(orig); self.library.selected_track = Some(0);
                    if !self.library.tracks.contains_key(&albums[orig].id) {
                        self.library.tracks.insert(albums[orig].id.clone(), LoadingState::Loading);
                        self.fetch_tracks(albums[orig].id.clone());
                    }
                }
            }
            BrowserColumn::Tracks => {
                let Some(album_id) = self.library.current_album().map(|a| a.id.clone()) else { return; };
                if let Some(LoadingState::Loaded(songs)) = self.library.tracks.get(&album_id) {
                    let indices: Vec<usize> = if let Some(q) = self.browser_column_filter(BrowserColumn::Tracks) {
                        songs.iter().enumerate().filter(|(_, s)| s.title.to_lowercase().contains(q)).map(|(i, _)| i).collect()
                    } else { (0..songs.len()).collect() };
                    if indices.is_empty() { return; }
                    let cur = self.library.selected_track.and_then(|sel| indices.iter().position(|&i| i == sel)).unwrap_or(0);
                    let page = self.browser_list_viewport_rows.max(1);
                    let new = match dir {
                        Direction::Up => cur.saturating_sub(line_steps), Direction::Down => (cur+line_steps).min(indices.len()-1),
                        Direction::Top => 0, Direction::Bottom => indices.len()-1,
                        Direction::PageUp => cur.saturating_sub(page), Direction::PageDown => (cur+page).min(indices.len()-1),
                    };
                    self.library.selected_track = Some(indices[new]);
                }
            }
        }
    }

    pub(crate) fn handle_navigate_browser_files(&mut self, dir: Direction, line_steps: usize) {
        let page = self.browser_list_viewport_rows.max(1);
        match self.browser_focus {
            BrowserColumn::Artists | BrowserColumn::Albums => {
                let len = if self.folders.path.is_empty() {
                    match &self.folders.roots {
                        LoadingState::Loaded(roots) => if let Some(q) = self.browser_column_filter(BrowserColumn::Artists) {
                            roots.iter().filter(|r| r.name.to_lowercase().contains(q)).count()
                        } else { roots.len() },
                        _ => return,
                    }
                } else {
                    match self.folders.current_listing() {
                        Some(LoadingState::Loaded(listing)) => {
                            let sub = if let Some(q) = self.browser_column_filter(BrowserColumn::Artists) {
                                listing.directories.iter().filter(|(_, n)| n.to_lowercase().contains(q)).count()
                            } else { listing.directories.len() };
                            1 + sub
                        }
                        _ => return,
                    }
                };
                if len == 0 { return; }
                let cur = self.folders.selected_dir.unwrap_or(0).min(len-1);
                self.folders.selected_dir = Some(match dir {
                    Direction::Up => cur.saturating_sub(line_steps), Direction::Down => (cur+line_steps).min(len-1),
                    Direction::Top => 0, Direction::Bottom => len-1,
                    Direction::PageUp => cur.saturating_sub(page), Direction::PageDown => (cur+page).min(len-1),
                });
                self.folders.folder_default_row_pending = false;
                self.sync_folder_preview_from_left();
            }
            BrowserColumn::Tracks => {
                let Some(pid) = self.folders.preview_dir_id.clone() else { return; };
                if let Some(LoadingState::Loaded(listing)) = self.folders.listings.get(&pid) {
                    let rows = crate::state::folder_preview_rows(listing, self.browser_column_filter(BrowserColumn::Tracks));
                    if rows.is_empty() { return; }
                    let cur_pos = self.folders.preview_selected_row.min(rows.len()-1);
                    self.folders.preview_selected_row = match dir {
                        Direction::Up => cur_pos.saturating_sub(line_steps), Direction::Down => (cur_pos+line_steps).min(rows.len()-1),
                        Direction::Top => 0, Direction::Bottom => rows.len()-1,
                        Direction::PageUp => cur_pos.saturating_sub(page), Direction::PageDown => (cur_pos+page).min(rows.len()-1),
                    };
                }
            }
        }
    }

    pub(crate) fn handle_add_to_queue(&mut self) {
        if self.browse_files() {
            if let Some(song) = self.folders.current_preview_track(self.browser_column_filter(BrowserColumn::Tracks)).cloned() {
                let was_empty = self.queue.songs.is_empty();
                self.queue.push(song);
                if was_empty { self.queue.cursor = 0; self.play_current(); }
            }
            return;
        }
        if let Some(song) = self.library.current_track().cloned() {
            let was_empty = self.queue.songs.is_empty();
            self.queue.push(song);
            if was_empty { self.queue.cursor = 0; self.play_current(); }
        }
    }

    pub(crate) fn handle_add_all_to_queue(&mut self, mode: AddAllMode) {
        if self.browse_files() {
            let Some(songs) = self.folder_preview_songs_for_queue() else { return; };
            let n = songs.len();
            let prepend = matches!(mode, AddAllMode::Prepend);
            match mode {
                AddAllMode::ReplaceAlbum | AddAllMode::ReplaceArtist => {
                    self.queue.songs = songs; self.queue.cursor = 0; self.queue.scroll = 0;
                    self.queue.pre_shuffle_order = None;
                    let _ = self.player_tx.send(PlayerCommand::Stop);
                    self.playback.current_song = None; self.playback.elapsed = Duration::ZERO;
                    self.playback.paused = false; self.playback.player_loaded = false;
                    if !self.queue.songs.is_empty() { self.play_current(); }
                    self.flash_queue_bulk_add(n, true);
                }
                AddAllMode::Append | AddAllMode::Prepend => {
                    let was_empty = self.queue.songs.is_empty();
                    if prepend { self.queue.prepend_songs(songs); }
                    else { for s in songs { self.queue.push(s); } }
                    if was_empty && !self.queue.songs.is_empty() { self.queue.cursor = 0; self.play_current(); }
                    self.flash_queue_bulk_add(n, false);
                }
            }
            return;
        }
        match mode {
            AddAllMode::ReplaceAlbum => self.handle_replace_queue_with_current_album(),
            AddAllMode::ReplaceArtist => self.handle_replace_queue_with_current_artist(),
            AddAllMode::Append | AddAllMode::Prepend => {
                let prepend = matches!(mode, AddAllMode::Prepend);
                match self.browser_focus {
                    BrowserColumn::Artists | BrowserColumn::Albums => {
                        if let Some(artist) = self.library.current_artist() {
                            self.fetch_all_tracks_for_artist(artist.id.clone(), self.queue.songs.is_empty(), prepend);
                        }
                    }
                    BrowserColumn::Tracks => {
                        let Some(album_id) = self.library.current_album().map(|a| a.id.clone()) else { return; };
                        if let Some(LoadingState::Loaded(songs)) = self.library.tracks.get(&album_id) {
                            let mut sorted = songs.clone();
                            sorted.sort_by_key(|s| (s.disc_number.unwrap_or(1), s.track.unwrap_or(0)));
                            let was_empty = self.queue.songs.is_empty();
                            if prepend { self.queue.prepend_songs(sorted); }
                            else { for s in sorted { self.queue.push(s); } }
                            if was_empty && !self.queue.songs.is_empty() { self.queue.cursor = 0; self.play_current(); }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn handle_confirm_library_index_append_queue(&mut self) {
        if !self.config.library_index_enabled { self.flash_status("Library index is disabled"); return; }
        if self.library_index_tracks.is_empty() { self.flash_status("Library index empty"); return; }
        let n = self.library_index_tracks.len();
        let was_empty = self.queue.songs.is_empty();
        for song in self.library_index_tracks.iter().cloned() { self.queue.push(song); }
        if was_empty && !self.queue.songs.is_empty() { self.queue.cursor = 0; self.queue.scroll = 0; self.play_current(); }
        self.flash_status_secs(if n == 1 { "Added 1 track".into() } else { format!("Added {n} tracks") }, 3);
    }

    pub(crate) fn handle_replace_queue_with_current_album(&mut self) {
        let Some(album_id) = self.library.current_album().map(|a| a.id.clone()) else { self.flash_status("Select an album"); return; };
        if let Some(LoadingState::Loaded(songs)) = self.library.tracks.get(&album_id) {
            let mut sorted = songs.clone();
            sorted.sort_by_key(|s| (s.disc_number.unwrap_or(1), s.track.unwrap_or(0)));
            self.handle_clear_queue();
            for s in sorted { self.queue.push(s); }
            if !self.queue.songs.is_empty() { self.queue.cursor = 0; self.queue.scroll = 0; self.play_current(); }
        }
    }

    pub(crate) fn handle_replace_queue_with_current_artist(&mut self) {
        if let Some(artist) = self.library.current_artist() {
            self.fetch_all_tracks_for_artist(artist.id.clone(), true, false);
        }
    }

    pub(crate) fn handle_search_confirm(&mut self) {
        let q = self.search_mode.query.to_lowercase();
        if q.is_empty() { return; }
        match self.active_tab {
            Tab::Home => {}
            Tab::Browser => {
                if self.browse_files() {
                    match self.browser_focus {
                        BrowserColumn::Artists | BrowserColumn::Albums => {
                            if self.folders.path.is_empty() {
                                if let LoadingState::Loaded(roots) = &self.folders.roots {
                                    if let Some(pos) = roots.iter().position(|r| r.name.to_lowercase().contains(&q)) {
                                        self.folders.selected_dir = Some(pos); self.folders.folder_default_row_pending = false;
                                        self.sync_folder_preview_from_left();
                                    }
                                }
                            } else if let Some(LoadingState::Loaded(listing)) = self.folders.current_listing() {
                                if let Some(pos) = listing.directories.iter().position(|(_, n)| n.to_lowercase().contains(&q)) {
                                    let child_id = listing.directories[pos].0.clone();
                                    self.folder_enter(child_id, listing.directories[pos].1.clone());
                                }
                            }
                        }
                        BrowserColumn::Tracks => {
                            if let Some(pid) = self.folders.preview_dir_id.clone() {
                                if let Some(LoadingState::Loaded(listing)) = self.folders.listings.get(&pid) {
                                    if let Some(pos) = listing.tracks.iter().position(|c| c.title.to_lowercase().contains(&q)) {
                                        self.folders.preview_selected_row = pos;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    match self.browser_focus {
                        BrowserColumn::Artists => {
                            if let LoadingState::Loaded(artists) = &self.library.artists {
                                if let Some(pos) = artists.iter().position(|a| a.name.to_lowercase().contains(&q)) {
                                    self.library.selected_artist = Some(pos); self.library.selected_album = Some(0); self.library.selected_track = Some(0);
                                }
                            }
                        }
                        BrowserColumn::Albums => {
                            if let Some(artist_id) = self.library.current_artist().map(|a| a.id.clone()) {
                                if let Some(LoadingState::Loaded(albums)) = self.library.albums.get(&artist_id) {
                                    if let Some(pos) = albums.iter().position(|a| a.name.to_lowercase().contains(&q)) {
                                        self.library.selected_album = Some(pos); self.library.selected_track = Some(0);
                                        if !self.library.tracks.contains_key(&albums[pos].id) {
                                            self.library.tracks.insert(albums[pos].id.clone(), LoadingState::Loading);
                                            self.fetch_tracks(albums[pos].id.clone());
                                        }
                                    }
                                }
                            }
                        }
                        BrowserColumn::Tracks => {
                            let Some(album_id) = self.library.current_album().map(|a| a.id.clone()) else { return; };
                            if let Some(LoadingState::Loaded(songs)) = self.library.tracks.get(&album_id) {
                                if let Some(pos) = songs.iter().position(|s| s.title.to_lowercase().contains(&q)) {
                                    self.library.selected_track = Some(pos);
                                }
                            }
                        }
                    }
                }
            }
            Tab::NowPlaying => {
                if let Some(pos) = self.queue.songs.iter().position(|s| s.title.to_lowercase().contains(&q)) {
                    self.queue.cursor = pos;
                    self.queue.scroll_clamp_cursor_visible(self.queue_viewport_rows.max(1));
                }
            }
            Tab::Playlists => {}
        }
    }

    pub(crate) fn handle_select(&mut self) {
        match self.active_tab {
            Tab::Home => self.handle_select_home(),
            Tab::Browser => {
                if self.browse_files() {
                    match self.browser_focus {
                        BrowserColumn::Artists | BrowserColumn::Albums => self.folder_activate_selected_dir(),
                        BrowserColumn::Tracks => self.folder_activate_preview_selection(),
                    }
                } else {
                    match self.browser_focus {
                        BrowserColumn::Artists | BrowserColumn::Albums => self.handle_focus_right(),
                        BrowserColumn::Tracks => self.handle_add_to_queue(),
                    }
                }
            }
            Tab::NowPlaying => { if !self.queue.songs.is_empty() { self.play_current(); } }
            Tab::Playlists => { if !self.playlists_tab.focus_left { self.handle_add_selected_playlist_track(); } }
        }
    }

    pub(crate) fn handle_navigate(&mut self, dir: Direction) {
        match self.active_tab {
            Tab::Home => self.handle_navigate_home(dir),
            Tab::Browser => self.handle_navigate_browser(dir, 1),
            Tab::NowPlaying => self.handle_navigate_queue(dir),
            Tab::Playlists => self.handle_navigate_playlists(dir),
        }
    }
}
