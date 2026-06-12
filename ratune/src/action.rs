#[derive(Debug, Clone)]
pub enum Direction {
    Up,
    Down,
    Top,    // gg (vim-style)
    Bottom, // G
    PageUp,
    PageDown,
}

use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Action {
    Navigate(Direction),
    Select,
    Back,
    /// Cycle tabs forward: Home → Browser → NowPlaying → Home (Tab key)
    SwitchTab,
    /// Cycle tabs backward: Home → NowPlaying → Browser → Home (Backtick / Shift+Tab)
    SwitchTabReverse,
    /// Jump directly to Home tab (key '1')
    GoToHome,
    /// Jump directly to Browser tab (key '2')
    GoToBrowser,
    /// Toggle Browse tab between artist columns and folder layout (requires config).
    ToggleBrowserFolder,
    /// Jump directly to NowPlaying tab (key '3')
    GoToNowPlaying,
    /// Jump directly to Playlists tab (key '4')
    GoToPlaylists,
    /// Playlists tab: scroll in left (list) panel
    PlaylistsScrollList,
    /// Playlists tab: scroll in right (tracks) panel
    PlaylistsScrollTracks,
    /// Playlists tab: append selected tracks to queue
    PlaylistsAddToQueue,
    /// Playlists tab: replace queue and play selected tracks
    PlaylistsPlayAll,
    /// Playlists tab: start text input for saving queue
    PlaylistsSaveQueue,
    /// Queue reorder: move current track up (Ctrl+Up)
    QueueMoveUp,
    /// Queue reorder: move current track down (Ctrl+Down)
    QueueMoveDown,
    /// Playlists tab: append selected tracks to queue
    PlaylistsAppend,
    /// Playlists tab: prepend selected tracks to queue
    PlaylistsPrepend,
    /// Playlists tab: insert selected track after current playing
    PlaylistsInsertNext,
    /// Playlists tab: toggle count (Favorites/Random)
    PlaylistsToggleCount,
    /// Playlists tab: delete selected track from playlist
    PlaylistsDeleteTrack,
    /// Playlists tab: confirm delete track
    PlaylistsConfirmDelete,
    /// Playlists tab: re-fetch random songs
    PlaylistsReRandom,
    /// Toggle repeat mode (none → one → all → none)
    ToggleRepeatMode,
    /// Toggle favorite (star) for current song
    ToggleFavorite,
    FocusLeft,
    FocusRight,
    AddToQueue,
    AddAllToQueue,
    /// Browser: replace queue with the **current album** (selected album + loaded tracks) and play.
    AddAllToQueueReplaceAlbum,
    /// Browser: replace queue with **all tracks for the current artist** (API fetch) and play.
    AddAllToQueueReplaceArtist,
    /// Browser: add all tracks — insert at the front of the queue.
    AddAllToQueuePrepend,
    PlayPause,
    NextTrack,
    PrevTrack,
    VolumeUp,
    VolumeDown,
    ClearQueue,
    /// Remove the highlighted track from the queue (Now Playing tab).
    RemoveFromQueue,
    Shuffle,
    Unshuffle,
    SeekForward,
    SeekBackward,
    /// Seek to an exact position (used by progress-bar clicks).
    SeekTo(Duration),
    SearchStart,
    SearchInput(char),
    SearchBackspace,
    SearchConfirm,
    SearchCancel,
    /// Toggle dynamic accent colour extraction from album art.
    ToggleDynamicTheme,
    /// Toggle the lyrics overlay on the NowPlaying tab.
    ToggleLyrics,
    /// Toggle the spectrum visualizer overlay on the NowPlaying tab.
    ToggleVisualizer,
    /// Toggle the keybind reference popup.
    ToggleHelp,
    /// Scroll the help popup up one line.
    HelpScrollUp,
    /// Scroll the help popup down one line.
    HelpScrollDown,
    /// Move to the next section on the Home tab (RecentAlbums → RecentTracks → TopArtists → Rediscover).
    HomeSectionNext,
    /// Move to the previous section on the Home tab.
    HomeSectionPrev,
    /// Refresh Home tab data (re-rolls rediscover suggestions).
    HomeRefresh,
    /// Navigate the art strip left (decrement selected album).
    HomeAlbumLeft,
    /// Navigate the art strip right (increment selected album).
    HomeAlbumRight,
    /// Add the selected album (strip) to queue, replacing existing queue.
    #[allow(dead_code)]
    HomeAlbumPlay,
    /// Append the selected album (strip) to queue without clearing.
    HomeAlbumAddToQueue,
    /// Open the fzf (or `sk`) track picker using the local metadata index.
    LibraryFzfPicker,
    /// Force a full refresh of the metadata index from Subsonic.
    LibraryIndexRefresh,
    /// Confirm a pending full library index refresh.
    ConfirmLibraryIndexRefresh,
    /// Propose appending all indexed library tracks to the queue (shows y/n first).
    LibraryIndexAppendQueue,
    /// Confirm pending append of the full metadata index to the queue.
    ConfirmLibraryIndexAppendQueue,
    /// Confirm pending append of the full server library (non-index) to the queue.
    ConfirmLibraryServerAppendQueue,
    /// Dismiss a global confirmation prompt (e.g. library refresh).
    CancelGlobalConfirm,
    Quit,
    None,
}
