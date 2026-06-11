//! Audio playback engine.
//!
//! Runs entirely on a dedicated `std::thread` — no tokio inside this module.
//! The TUI communicates via two `std::sync::mpsc` channels:
//!
//! - `PlayerCommand` (TUI → engine): play a URL, pause, resume, stop, set volume.
//! - `PlayerEvent`  (engine → TUI): progress ticks, track-ended, errors.

use std::collections::VecDeque;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::num::NonZero;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::ACCEPT_ENCODING;
use magnum::container::ogg::OpusSourceOgg;
use rodio::source::SeekError;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, Player, SampleRate, Source};

use crate::tap::SampleTap;

type SampleBuffer = Arc<Mutex<VecDeque<f32>>>;

/// Adapts magnum's OpusSourceOgg to rodio 0.22's Source trait.
///
/// magnum's built-in `with_rodio` feature targets rodio 0.14 and is not
/// compatible with the 0.22 version used elsewhere in this crate.
struct MagnumOpusSource {
    inner: OpusSourceOgg<Cursor<Vec<u8>>>,
}

impl Iterator for MagnumOpusSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.inner.next()
    }
}

impl Source for MagnumOpusSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(240) // Opus: 120 samples/channel × 2 channels
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(self.inner.metadata.channel_count.into()).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(48_000u32).unwrap() // Opus always decodes at 48 kHz
    }

    fn total_duration(&self) -> Option<Duration> {
        None // magnum does not calculate total duration
    }

    fn try_seek(&mut self, _pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: "magnum OpusSourceOgg",
        })
    }
}

/// Unified source type: Symphonia for most formats, magnum for Opus Ogg.
enum DecodedSource {
    Symphonia(Decoder<Cursor<Vec<u8>>>),
    MagnumOpus(MagnumOpusSource),
}

impl Iterator for DecodedSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        match self {
            Self::Symphonia(d) => d.next(),
            Self::MagnumOpus(d) => d.next(),
        }
    }
}

impl Source for DecodedSource {
    fn current_span_len(&self) -> Option<usize> {
        match self {
            Self::Symphonia(d) => d.current_span_len(),
            Self::MagnumOpus(d) => d.current_span_len(),
        }
    }

    fn channels(&self) -> ChannelCount {
        match self {
            Self::Symphonia(d) => d.channels(),
            Self::MagnumOpus(d) => d.channels(),
        }
    }

    fn sample_rate(&self) -> SampleRate {
        match self {
            Self::Symphonia(d) => d.sample_rate(),
            Self::MagnumOpus(d) => d.sample_rate(),
        }
    }

    fn total_duration(&self) -> Option<Duration> {
        match self {
            Self::Symphonia(d) => d.total_duration(),
            Self::MagnumOpus(d) => d.total_duration(),
        }
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match self {
            Self::Symphonia(d) => d.try_seek(pos),
            Self::MagnumOpus(d) => d.try_seek(pos),
        }
    }
}

// ── Public channel types ──────────────────────────────────────────────────────

/// Commands sent from the TUI to the player thread.
#[derive(Debug)]
pub enum PlayerCommand {
    /// Start playing the track at `url`. `duration` is the expected total
    /// duration (from Subsonic metadata), used for progress display.
    /// `gen` is a monotonically increasing counter from the TUI; the engine
    /// uses it to discard stale downloads when multiple skips arrive quickly.
    PlayUrl {
        url: String,
        duration: Option<Duration>,
        gen: u64,
    },
    /// Same semantics as [`PlayUrl`](Self::PlayUrl), but reads audio from a local cache file.
    PlayCached {
        path: PathBuf,
        duration: Option<Duration>,
        gen: u64,
    },
    /// Append the next track to the player queue for gapless playback.
    ///
    /// Must only be sent in response to `PlayerEvent::AboutToFinish`.
    /// Does NOT stop current playback.
    EnqueueNext {
        url: String,
        duration: Option<Duration>,
    },
    /// Gapless prefetch for an offline-cached next track (see [`PlayCached`](Self::PlayCached)).
    EnqueueNextCached {
        path: PathBuf,
        duration: Option<Duration>,
    },
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    /// Seek to an absolute position in the current track.
    Seek(Duration),
    /// Stop playback and shut down the player thread cleanly.
    Quit,
}

/// Events sent from the player thread back to the TUI.
#[derive(Debug)]
pub enum PlayerEvent {
    TrackStarted,
    /// Fired every ~500 ms. `total` is `None` when unknown.
    Progress {
        elapsed: Duration,
        total: Option<Duration>,
    },
    /// Fired ~5 s before the current track ends. The TUI should respond with
    /// `PlayerCommand::EnqueueNext` to enable gapless playback.
    AboutToFinish,
    /// Fired when a gaplessly enqueued track begins playing (elapsed resets).
    TrackAdvanced,
    TrackEnded,
    Error(String),
}

// ── Engine spawn ──────────────────────────────────────────────────────────────

/// Spawn the player thread.
///
/// Returns `(cmd_tx, evt_rx, join_handle, sample_buffer)`.  The caller should send
/// `PlayerCommand::Quit` and then join the handle (with a timeout) on
/// shutdown to ensure the audio device is released cleanly.
///
/// `sample_buffer` is a ring buffer of the most recent decoded f32 samples;
/// the TUI reads it each frame to drive the visualizer FFT.
pub fn spawn_player() -> (
    mpsc::Sender<PlayerCommand>,
    mpsc::Receiver<PlayerEvent>,
    std::thread::JoinHandle<()>,
    SampleBuffer,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
    let (evt_tx, evt_rx) = mpsc::channel::<PlayerEvent>();

    let sample_buffer: SampleBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
    let thread_buffer = sample_buffer.clone();

    let handle = std::thread::Builder::new()
        .name("ratune-player".into())
        .spawn(move || player_thread(cmd_rx, evt_tx, thread_buffer))
        .expect("failed to spawn player thread");

    (cmd_tx, evt_rx, handle, sample_buffer)
}

// ── Player thread ─────────────────────────────────────────────────────────────

fn player_thread(
    cmd_rx: mpsc::Receiver<PlayerCommand>,
    evt_tx: mpsc::Sender<PlayerEvent>,
    sample_buffer: SampleBuffer,
) {
    // MixerDeviceSink must live for the duration of playback.
    let mut device = match DeviceSinkBuilder::open_default_sink() {
        Ok(d) => d,
        Err(e) => {
            let _ = evt_tx.send(PlayerEvent::Error(format!("audio device error: {e}")));
            return;
        }
    };
    // Suppress the default stderr message on drop — we control shutdown explicitly.
    device.log_on_drop(false);

    let player = Player::connect_new(device.mixer());

    // State for the current track.
    let mut current_total: Option<Duration> = None;
    // Tracks whether the previous tick saw a non-empty player (to detect natural end).
    let mut was_playing = false;
    // Gapless state.
    let mut next_total: Option<Duration> = None;
    let mut next_queued = false;
    let mut about_to_finish_sent = false;
    let mut prev_elapsed = Duration::ZERO;
    // Skip-generation counter: updated every time a PlayUrl / PlayCached is received.
    // Used to discard stale loads when the user skips rapidly.
    let mut skip_gen: u64 = 0;

    'outer: loop {
        // ── Drain all pending commands (non-blocking) ─────────────────────────
        loop {
            use mpsc::TryRecvError;
            match cmd_rx.try_recv() {
                Ok(PlayerCommand::Quit) => break 'outer,
                Ok(PlayerCommand::PlayUrl { url, duration, gen }) => {
                    play_payload(
                        PlayPayload::Url(url),
                        duration,
                        gen,
                        &cmd_rx,
                        &mut skip_gen,
                        &player,
                        &evt_tx,
                        &mut current_total,
                        &mut was_playing,
                        &mut next_total,
                        &mut next_queued,
                        &mut about_to_finish_sent,
                        &mut prev_elapsed,
                        &sample_buffer,
                    );
                }
                Ok(PlayerCommand::PlayCached {
                    path,
                    duration,
                    gen,
                }) => {
                    play_payload(
                        PlayPayload::Cached(path),
                        duration,
                        gen,
                        &cmd_rx,
                        &mut skip_gen,
                        &player,
                        &evt_tx,
                        &mut current_total,
                        &mut was_playing,
                        &mut next_total,
                        &mut next_queued,
                        &mut about_to_finish_sent,
                        &mut prev_elapsed,
                        &sample_buffer,
                    );
                }
                Ok(cmd) => handle_command(
                    cmd,
                    &player,
                    &evt_tx,
                    &mut current_total,
                    &mut was_playing,
                    &mut next_total,
                    &mut next_queued,
                    &mut about_to_finish_sent,
                    &mut prev_elapsed,
                    &sample_buffer,
                ),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'outer,
            }
        }

        // ── Progress tick ─────────────────────────────────────────────────────
        if !player.is_paused() && !player.empty() {
            let elapsed = player.get_pos();

            // Detect gapless track transition: elapsed resets to near zero
            // while we know a next track was appended.  Use a 2 s window rather
            // than 500 ms to tolerate rodio's first-tick imprecision.
            if next_queued
                && prev_elapsed > Duration::from_secs(2)
                && elapsed < Duration::from_secs(2)
            {
                current_total = next_total.take();
                next_queued = false;
                about_to_finish_sent = false;
                let _ = evt_tx.send(PlayerEvent::TrackAdvanced);
            }
            prev_elapsed = elapsed;

            let _ = evt_tx.send(PlayerEvent::Progress {
                elapsed,
                total: current_total,
            });

            // Send AboutToFinish ~10 s before the end so the TUI can enqueue next.
            // 10 s gives enough headroom for: player-thread sleep (≤500 ms) +
            // TUI dispatch latency + full-track download + decode.
            if !about_to_finish_sent && !next_queued {
                if let Some(total) = current_total {
                    let remaining = total.saturating_sub(elapsed);
                    if remaining <= Duration::from_secs(10) && remaining > Duration::ZERO {
                        about_to_finish_sent = true;
                        let _ = evt_tx.send(PlayerEvent::AboutToFinish);
                    }
                }
            }

            was_playing = true;
        }

        // ── Natural track end detection (no next track was enqueued) ──────────
        if was_playing && player.empty() {
            was_playing = false;
            current_total = None;
            next_total = None;
            next_queued = false;
            about_to_finish_sent = false;
            prev_elapsed = Duration::ZERO;
            let _ = evt_tx.send(PlayerEvent::TrackEnded);
        }

        std::thread::sleep(Duration::from_millis(500));
    }

    // Stop playback before releasing the audio device.
    player.stop();
    drop(player);
    drop(device);
}

#[derive(Debug, Clone)]
enum PlayPayload {
    Url(String),
    Cached(PathBuf),
}

/// Handle [`PlayerCommand::PlayUrl`] / [`PlayerCommand::PlayCached`] with skip-generation cancellation.
///
/// Before loading, drains any further play commands already queued — turning N rapid
/// skips into one fetch/read. After the blocking load, drains again; if a newer play
/// command arrived mid-load, discards the decoder and recurses.
#[allow(clippy::too_many_arguments)]
fn play_payload(
    payload: PlayPayload,
    duration: Option<Duration>,
    gen: u64,
    cmd_rx: &mpsc::Receiver<PlayerCommand>,
    skip_gen: &mut u64,
    player: &Player,
    evt_tx: &mpsc::Sender<PlayerEvent>,
    current_total: &mut Option<Duration>,
    was_playing: &mut bool,
    next_total: &mut Option<Duration>,
    next_queued: &mut bool,
    about_to_finish_sent: &mut bool,
    prev_elapsed: &mut Duration,
    sample_buffer: &SampleBuffer,
) {
    *skip_gen = gen;

    // ── Pre-load drain ────────────────────────────────────────────────────────
    let mut final_payload = payload;
    let mut final_duration = duration;
    let mut final_gen = gen;
    loop {
        match cmd_rx.try_recv() {
            Ok(PlayerCommand::PlayUrl {
                url: u,
                duration: d,
                gen: g,
            }) => {
                final_payload = PlayPayload::Url(u);
                final_duration = d;
                final_gen = g;
                *skip_gen = g;
            }
            Ok(PlayerCommand::PlayCached {
                path: p,
                duration: d,
                gen: g,
            }) => {
                final_payload = PlayPayload::Cached(p);
                final_duration = d;
                final_gen = g;
                *skip_gen = g;
            }
            Ok(_other) => break,
            Err(_) => break,
        }
    }

    player.stop();
    *was_playing = false;
    *next_total = None;
    *next_queued = false;
    *about_to_finish_sent = false;
    *prev_elapsed = Duration::ZERO;

    // ── Load (network or disk) ──────────────────────────────────────────────
    let source = match &final_payload {
        PlayPayload::Url(url) => match download_and_decode(url) {
            Ok(s) => s,
            Err(e) => {
                let _ = evt_tx.send(PlayerEvent::Error(format!("playback error: {e}")));
                return;
            }
        },
        PlayPayload::Cached(path) => match read_cached_and_decode(path) {
            Ok(s) => s,
            Err(e) => {
                let _ = evt_tx.send(PlayerEvent::Error(format!("playback error: {e}")));
                return;
            }
        },
    };

    // ── Post-load drain ───────────────────────────────────────────────────────
    let mut newer: Option<(PlayPayload, Option<Duration>, u64)> = None;
    loop {
        match cmd_rx.try_recv() {
            Ok(PlayerCommand::PlayUrl {
                url: u,
                duration: d,
                gen: g,
            }) => {
                *skip_gen = g;
                newer = Some((PlayPayload::Url(u), d, g));
            }
            Ok(PlayerCommand::PlayCached {
                path: p,
                duration: d,
                gen: g,
            }) => {
                *skip_gen = g;
                newer = Some((PlayPayload::Cached(p), d, g));
            }
            Ok(_other) => break,
            Err(_) => break,
        }
    }

    if *skip_gen != final_gen {
        drop(source);
        if let Some((p, d, g)) = newer {
            play_payload(
                p,
                d,
                g,
                cmd_rx,
                skip_gen,
                player,
                evt_tx,
                current_total,
                was_playing,
                next_total,
                next_queued,
                about_to_finish_sent,
                prev_elapsed,
                sample_buffer,
            );
        }
        return;
    }

    *current_total = final_duration;
    let tapped = SampleTap::new(source, sample_buffer.clone());
    player.append(tapped);
    player.play();
    let _ = evt_tx.send(PlayerEvent::TrackStarted);
}

#[allow(clippy::too_many_arguments)] // Engine thread: one place for all command side-effects.
fn handle_command(
    cmd: PlayerCommand,
    player: &Player,
    evt_tx: &mpsc::Sender<PlayerEvent>,
    current_total: &mut Option<Duration>,
    was_playing: &mut bool,
    next_total: &mut Option<Duration>,
    next_queued: &mut bool,
    about_to_finish_sent: &mut bool,
    prev_elapsed: &mut Duration,
    sample_buffer: &SampleBuffer,
) {
    match cmd {
        PlayerCommand::PlayUrl { .. } | PlayerCommand::PlayCached { .. } => {
            unreachable!("PlayUrl / PlayCached must be dispatched via play_payload()");
        }
        PlayerCommand::EnqueueNext { url, duration } => match download_and_decode(&url) {
            Ok(source) => {
                *next_total = duration;
                *next_queued = true;
                let tapped = SampleTap::new(source, sample_buffer.clone());
                player.append(tapped);
            }
            Err(e) => {
                let _ = evt_tx.send(PlayerEvent::Error(format!("enqueue error: {e}")));
            }
        },
        PlayerCommand::EnqueueNextCached { path, duration } => {
            match read_cached_and_decode(&path) {
                Ok(source) => {
                    *next_total = duration;
                    *next_queued = true;
                    let tapped = SampleTap::new(source, sample_buffer.clone());
                    player.append(tapped);
                }
                Err(e) => {
                    let _ = evt_tx.send(PlayerEvent::Error(format!("enqueue error: {e}")));
                }
            }
        }
        PlayerCommand::Pause => player.pause(),
        PlayerCommand::Resume => player.play(),
        PlayerCommand::Stop => {
            player.stop();
            *current_total = None;
            *next_total = None;
            *next_queued = false;
            *about_to_finish_sent = false;
            *prev_elapsed = Duration::ZERO;
            *was_playing = false;
        }
        PlayerCommand::SetVolume(v) => player.set_volume(v),
        PlayerCommand::Seek(pos) => {
            let _ = player.try_seek(pos);
            // Update prev_elapsed so the gapless-transition heuristic isn't
            // confused by the sudden position jump.
            *prev_elapsed = pos;
        }
        PlayerCommand::Quit => {
            // Handled by the 'outer break in player_thread — should not reach here.
            unreachable!("Quit must be handled in the outer command-drain loop");
        }
    }
}

// ── Stream + decode ───────────────────────────────────────────────────────────

/// Shared client: default `get()` uses short timeouts and can truncate large or
/// slow streams; long tracks need a generous read deadline and occasional retries.
fn stream_http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            // Whole request including body (large lossless files over slow links).
            .timeout(Duration::from_secs(900))
            .connect_timeout(Duration::from_secs(60))
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .user_agent(concat!("ratune/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("stream HTTP client")
    })
}

fn fetch_track_bytes(url: &str, accept_identity: bool) -> Result<Vec<u8>> {
    let mut req = stream_http_client().get(url);
    if accept_identity {
        // Some proxies / servers serve odd combinations of Content-Encoding and
        // body bytes; asking for identity avoids reqwest's decompress errors on
        // those tracks while still returning raw audio.
        req = req.header(ACCEPT_ENCODING, "identity");
    }
    let response = req.send().context("connecting to stream URL")?;
    let status = response.status();
    let response = response
        .error_for_status()
        .with_context(|| format!("stream HTTP {status}"))?;
    let bytes = response
        .bytes()
        .context("reading stream body (connection dropped or truncated?)")?;
    Ok(bytes.to_vec())
}

fn read_cached_and_decode(path: &std::path::Path) -> Result<DecodedSource> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading cached track {}", path.display()))?;
    build_decoder(bytes)
}

fn build_decoder(bytes: Vec<u8>) -> Result<DecodedSource> {
    // Quick check for Opus in Ogg container: header starts with "OggS" and
    // the identification packet's magic is "OpusHead".
    let is_opus = bytes.len() > 36
        && bytes.starts_with(b"OggS")
        && bytes.windows(8).any(|w| w == b"OpusHead");

    if is_opus {
        let cursor = Cursor::new(bytes);
        let source = OpusSourceOgg::new(cursor)
            .context("decoding Opus audio with magnum")?;
        return Ok(DecodedSource::MagnumOpus(MagnumOpusSource { inner: source }));
    }

    // Fall back to Symphonia for all other formats (MP3, FLAC, Vorbis, AAC, etc.).
    let byte_len = bytes.len() as u64;
    let cursor = Cursor::new(bytes);
    Decoder::builder()
        .with_data(cursor)
        .with_byte_len(byte_len)
        .with_coarse_seek(true)
        .build()
        .map(DecodedSource::Symphonia)
        .context("decoding audio (unsupported or corrupt file?)")
}

fn download_and_decode(url: &str) -> Result<DecodedSource> {
    // Download the full track into RAM so symphonia gets an unambiguously
    // seekable Cursor<Vec<u8>>.  StreamingReader over a BufReader was technically
    // seekable but symphonia's demuxer cached seekability as false during probe,
    // causing every try_seek to return Err(Unseekable).
    //
    // with_byte_len: tells symphonia the exact byte length, which also sets
    //   is_seekable = true internally.
    // with_coarse_seek: bypasses the time_base requirement for accurate seeking
    //   (unavailable on transcoded MP3 streams); seeks to the nearest keyframe.
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..4u32 {
        if attempt > 0 {
            thread::sleep(Duration::from_millis(200 * u64::from(attempt)));
        }
        // After a few normal tries (compressed transfer), ask for identity once.
        let identity = attempt == 3;
        match fetch_track_bytes(url, identity).and_then(build_decoder) {
            Ok(decoder) => return Ok(decoder),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("loop runs at least once"))
}
