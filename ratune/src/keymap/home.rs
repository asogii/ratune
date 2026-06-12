use crossterm::event::{KeyCode, KeyModifiers};

use crate::action::Action;
use crate::keybinds::Keybinds;

/// Home-tab-specific key mappings.
pub fn map_key(code: KeyCode, modifiers: KeyModifiers, kb: &Keybinds) -> Action {
    if kb.home_section_next.matches(code, modifiers) {
        return Action::HomeSectionNext;
    }
    // Extra Home-section aliases: Shift+h / Shift+l (sent as H/L or h/l+SHIFT).
    if (code == KeyCode::Char('H') && modifiers.is_empty())
        || (code == KeyCode::Char('h') && modifiers.intersects(KeyModifiers::SHIFT))
    {
        return Action::HomeSectionPrev;
    }
    if (code == KeyCode::Char('L') && modifiers.is_empty())
        || (code == KeyCode::Char('l') && modifiers.intersects(KeyModifiers::SHIFT))
    {
        return Action::HomeSectionNext;
    }
    if kb.home_section_prev.matches(code, modifiers) {
        return Action::HomeSectionPrev;
    }
    if kb.home_refresh.matches(code, modifiers) {
        return Action::HomeRefresh;
    }
    // Ctrl+r: replace queue with the selected album.
    if let Some(spec) = &kb.add_all_replace_album {
        if spec.matches(code, modifiers) {
            return Action::HomeAlbumPlay;
        }
    }
    if kb.column_left.matches(code, modifiers) {
        return Action::HomeAlbumLeft;
    }
    if kb.column_right.matches(code, modifiers) {
        return Action::HomeAlbumRight;
    }
    if kb.add_track.matches(code, modifiers) {
        return Action::HomeAlbumAddToQueue;
    }
    Action::None
}
