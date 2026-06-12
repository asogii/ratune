//! Per-tab handler modules.
//!
//! Each sub-module contains `impl crate::app::App { ... }` blocks that
//! implement the handler functions called by the main dispatch in `app.rs`.

pub mod browser;
pub mod home;
pub mod playlists;
pub mod queue;
