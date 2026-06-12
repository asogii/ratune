use crossterm::event::{KeyCode, KeyModifiers};

use crate::action::Action;

/// Browser-tab-specific key mappings.
pub fn map_key(code: KeyCode, modifiers: KeyModifiers, kb: &Keybinds) -> Action {
    Action::None
}
