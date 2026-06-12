pub mod browser;
pub mod home;
pub mod playlists;

use crate::action::{Action, Direction};
use crate::app::Tab;
use crate::keybinds::Keybinds;
use crossterm::event::{KeyCode, KeyModifiers};

/// Dispatch to the active tab's keymap, then fall back to always-on keys.
pub fn map_key(
    code: KeyCode,
    modifiers: KeyModifiers,
    active_tab: Tab,
    kb: &Keybinds,
    pending_gg: &mut bool,
) -> Action {
    // ── Second `g` after a lone `g`: vim-style `gg` → top ────────────────────
    if *pending_gg {
        *pending_gg = false;
        if code == KeyCode::Char('g') && modifiers.is_empty() {
            return Action::Navigate(Direction::Top);
        }
    }

    // ── Tab-specific keys ─────────────────────────────────────────────────────
    let tab_action = match active_tab {
        Tab::Home => home::map_key(code, modifiers, kb),
        Tab::Browser => browser::map_key(code, modifiers, kb),
        Tab::Playlists => playlists::map_key(code, modifiers),
        Tab::NowPlaying => {
            // NowPlaying has only `s` as a tab-specific key.
            if code == KeyCode::Char('s') && modifiers.is_empty() {
                Action::PlaylistsSaveQueue
            } else {
                Action::None
            }
        }
    };
    if !matches!(tab_action, Action::None) {
        return tab_action;
    }

    // ── Always-on / non-configurable ─────────────────────────────────────────
    always_on(code, modifiers, active_tab, kb, pending_gg)
}

/// Keys that work regardless of active tab.
fn always_on(
    code: KeyCode,
    modifiers: KeyModifiers,
    active_tab: Tab,
    kb: &Keybinds,
    pending_gg: &mut bool,
) -> Action {
    // G: jump to bottom — not exposed in config. Top is `gg` (handled via pending_gg).
    if code == KeyCode::Char('G')
        && !modifiers.intersects(
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER,
        )
    {
        return Action::Navigate(Direction::Bottom);
    }
    // Enter / Esc
    if code == KeyCode::Enter {
        return Action::Select;
    }
    if code == KeyCode::Esc {
        return Action::Back;
    }
    // Space alone is an alias for play_pause.
    if code == KeyCode::Char(' ') && modifiers.is_empty() {
        return Action::PlayPause;
    }
    // '=' is always a secondary alias for volume_up.
    if code == KeyCode::Char('=') {
        return Action::VolumeUp;
    }

    if kb.toggle_help.matches(code, modifiers) {
        return Action::ToggleHelp;
    }
    if kb.toggle_dynamic_theme.matches(code, modifiers) {
        return Action::ToggleDynamicTheme;
    }
    if kb.toggle_lyrics.matches(code, modifiers) {
        return Action::ToggleLyrics;
    }
    if kb.toggle_visualizer.matches(code, modifiers)
        || code == KeyCode::Char('V')
        || (code == KeyCode::Char('v') && modifiers.intersects(KeyModifiers::SHIFT))
    {
        return Action::ToggleVisualizer;
    }

    // Up/Down arrows
    if code == KeyCode::Up
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Action::Navigate(Direction::Up);
    }
    if code == KeyCode::Down
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Action::Navigate(Direction::Down);
    }

    // PageUp/PageDown and Ctrl+u / Ctrl+d
    {
        let ctrl = modifiers.intersects(KeyModifiers::CONTROL)
            && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SHIFT);
        if code == KeyCode::PageUp
            || (ctrl && matches!(code, KeyCode::Char('u') | KeyCode::Char('U')))
        {
            return Action::Navigate(Direction::PageUp);
        }
        if code == KeyCode::PageDown
            || (ctrl && matches!(code, KeyCode::Char('d') | KeyCode::Char('D')))
        {
            return Action::Navigate(Direction::PageDown);
        }
    }

    // ── Configurable keybinds ─────────────────────────────────────────────────
    if kb.quit.matches(code, modifiers) {
        return Action::Quit;
    }
    if kb.tab_switch.matches(code, modifiers) {
        return Action::SwitchTab;
    }
    if kb.tab_switch_reverse.matches(code, modifiers) {
        return Action::SwitchTabReverse;
    }
    if code == KeyCode::BackTab {
        return Action::SwitchTabReverse;
    }
    if kb.go_to_home.matches(code, modifiers) {
        return Action::GoToHome;
    }
    if kb.go_to_browser.matches(code, modifiers) {
        return Action::GoToBrowser;
    }
    if kb.go_to_nowplaying.matches(code, modifiers) {
        return Action::GoToNowPlaying;
    }
    if kb.go_to_playlists.matches(code, modifiers) {
        return Action::GoToPlaylists;
    }
    if let Some(spec) = &kb.toggle_folder_browse {
        if spec.matches(code, modifiers) {
            return Action::ToggleBrowserFolder;
        }
    }

    // seek_forward / seek_backward
    if kb.seek_forward.matches(code, modifiers) {
        return match active_tab {
            Tab::NowPlaying => Action::SeekForward,
            Tab::Browser | Tab::Home | Tab::Playlists => Action::FocusRight,
        };
    }
    if kb.seek_backward.matches(code, modifiers) {
        return match active_tab {
            Tab::NowPlaying => Action::SeekBackward,
            Tab::Browser | Tab::Home | Tab::Playlists => Action::FocusLeft,
        };
    }

    if kb.column_left.matches(code, modifiers) {
        return Action::FocusLeft;
    }
    if kb.column_right.matches(code, modifiers) {
        return Action::FocusRight;
    }
    if kb.scroll_up.matches(code, modifiers) {
        return Action::Navigate(Direction::Up);
    }
    if kb.scroll_down.matches(code, modifiers) {
        return Action::Navigate(Direction::Down);
    }

    if kb.play_pause.matches(code, modifiers) {
        return Action::PlayPause;
    }
    if kb.next_track.matches(code, modifiers) {
        return Action::NextTrack;
    }
    if kb.prev_track.matches(code, modifiers) {
        return Action::PrevTrack;
    }

    // add_all variants — only in Browser tab.
    if active_tab == Tab::Browser {
        if let Some(spec) = &kb.add_all_replace_artist {
            if spec.matches(code, modifiers) {
                return Action::AddAllToQueueReplaceArtist;
            }
        }
        if let Some(spec) = &kb.add_all_replace_album {
            if spec.matches(code, modifiers) {
                return Action::AddAllToQueueReplaceAlbum;
            }
        }
        if let Some(spec) = &kb.add_all_prepend {
            if spec.matches(code, modifiers) {
                return Action::AddAllToQueuePrepend;
            }
        }
        if kb.add_all.matches(code, modifiers) {
            return Action::AddAllToQueue;
        }
        if kb.add_track.matches(code, modifiers) {
            return Action::AddToQueue;
        }
    }

    if kb.shuffle.matches(code, modifiers) {
        return Action::Shuffle;
    }
    if kb.unshuffle.matches(code, modifiers) {
        return Action::Unshuffle;
    }
    if kb.toggle_repeat_mode.matches(code, modifiers) {
        return Action::ToggleRepeatMode;
    }
    if kb.toggle_favorite.matches(code, modifiers) {
        return Action::ToggleFavorite;
    }

    // Queue re-order — works in NowPlaying & Playlists.
    if (active_tab == Tab::NowPlaying || active_tab == Tab::Playlists)
        && kb.queue_move_up.matches(code, modifiers)
    {
        return Action::QueueMoveUp;
    }
    if (active_tab == Tab::NowPlaying || active_tab == Tab::Playlists)
        && kb.queue_move_down.matches(code, modifiers)
    {
        return Action::QueueMoveDown;
    }

    if kb.clear_queue.matches(code, modifiers) {
        return Action::ClearQueue;
    }
    if active_tab == Tab::NowPlaying && kb.remove_from_queue.matches(code, modifiers) {
        return Action::RemoveFromQueue;
    }
    if kb.search.matches(code, modifiers) {
        return Action::SearchStart;
    }
    if kb.volume_up.matches(code, modifiers) {
        return Action::VolumeUp;
    }
    if kb.volume_down.matches(code, modifiers) {
        return Action::VolumeDown;
    }

    if let Some(spec) = &kb.library_fzf {
        if spec.matches(code, modifiers) {
            return Action::LibraryFzfPicker;
        }
    }
    if let Some(spec) = &kb.library_refresh {
        if spec.matches(code, modifiers) {
            return Action::LibraryIndexRefresh;
        }
    }
    if let Some(spec) = &kb.library_index_append_queue {
        if spec.matches(code, modifiers) {
            return Action::LibraryIndexAppendQueue;
        }
    }

    // Lone `g`: wait for second `g` (`gg`) to go to top (vim-style).
    if code == KeyCode::Char('g') && modifiers.is_empty() {
        *pending_gg = true;
        return Action::None;
    }

    Action::None
}
