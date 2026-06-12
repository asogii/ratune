use crossterm::event::{KeyCode, KeyModifiers};

use crate::action::Action;
use crate::keybinds::Keybinds;

/// Browser-tab-specific key mappings.
pub fn map_key(code: KeyCode, modifiers: KeyModifiers, kb: &Keybinds) -> Action {
    if kb.playlist_overlay.matches(code, modifiers) {
        return Action::TogglePlaylistOverlay;
    }
    if kb.browser_add_to_playlist.matches(code, modifiers) {
        return Action::BrowserAddToPlaylist;
    }
    Action::None
}
