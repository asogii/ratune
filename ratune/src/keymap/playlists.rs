use crossterm::event::{KeyCode, KeyModifiers};

use crate::action::Action;

/// Playlists-tab-specific key mappings.
pub fn map_key(code: KeyCode, modifiers: KeyModifiers) -> Action {
    // a: append to queue (right panel track)
    if code == KeyCode::Char('a') && modifiers.is_empty() {
        return Action::PlaylistsAppend;
    }
    // p: prepend to queue
    if code == KeyCode::Char('p') && modifiers.is_empty() {
        return Action::PlaylistsPrepend;
    }
    // i: insert next
    if code == KeyCode::Char('i') && modifiers.is_empty() {
        return Action::PlaylistsInsertNext;
    }
    // e: toggle count
    if code == KeyCode::Char('e') && modifiers.is_empty() {
        return Action::PlaylistsToggleCount;
    }
    // d: delete track/playlist
    if code == KeyCode::Char('d') && modifiers.is_empty() {
        return Action::PlaylistsDeleteTrack;
    }
    // s: save playlist
    if code == KeyCode::Char('s') && modifiers.is_empty() {
        return Action::PlaylistsSaveQueue;
    }
    // r: re-random
    if code == KeyCode::Char('r') && modifiers.is_empty() {
        return Action::PlaylistsReRandom;
    }
    Action::None
}
