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
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::header::ACCEPT_ENCODING;
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use symphonia::core::audio::SampleBuffer as SymphoniaSampleBuffer;
use symphonia::core::codecs::CodecRegistry;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;
use symphonia_adapter_libopus::OpusDecoder;
use rodio::source::SeekError;

use crate::tap::SampleTap;

type SampleBuffer = Arc<Mutex<VecDeque<f32>>>;

// ── Symphonia + libopus decoder (Opus Ogg) ──────────────────────────────────────

/// Adapter that wraps a `Cursor<Vec<u8>>` as a `MediaSource` for symphonia.
struct CursorMediaSource {
    inner: Cursor<Vec<u8>>,
    byte_len: u64,
}

impl CursorMediaSource {
    fn new(inner: Cursor<Vec<u8>>) -> Self {
        let byte_len = inner.get_ref().len() as u64;
        Self { inner, byte_len }
    }
}

impl std::io::Read for CursorMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl std::io::Seek for CursorMediaSource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl MediaSource for CursorMediaSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.byte_len)
    }
}

/// Symphonia-based Opus decoder that implements rodio's `Source`.
/// Uses `symphonia-adapter-libopus` for the actual Opus decoding.
struct SymphoniaOpusSource {
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    format: Box<dyn FormatReader>,
    buffer: SymphoniaSampleBuffer<f32>,
    buffer_offset: usize,
    spec: SignalSpec,
    total_duration: Option<Duration>,
}

impl SymphoniaOpusSource {
    fn new(bytes: Vec<u8>) -> Result<Self> {
        let mss = MediaSourceStream::new(
            Box::new(CursorMediaSource::new(Cursor::new(bytes))) as Box<dyn MediaSource>,
            Default::default(),
        );

        // Build a custom codec registry with just the Opus decoder.
        let mut codec_registry = CodecRegistry::new();
        codec_registry.register_all::<OpusDecoder>();

        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let hint = Hint::new();

        let mut probed = get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .map_err(|e| anyhow::anyhow!("symphonia probe failed: {e}"))?;

        // Find the first track with a supported codec.
        let track_id = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow::anyhow!("Opus track not found"))?
            .id;
        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.id == track_id)
            .ok_or_else(|| anyhow::anyhow!("Opus track not found"))?;

        let mut decoder = codec_registry
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| anyhow::anyhow!("Opus decoder init failed: {e}"))?;

        let total_duration = track
            .codec_params
            .time_base
            .zip(track.codec_params.n_frames)
            .map(|(base, frames)| base.calc_time(frames).into())
            .filter(|d: &Duration| !d.is_zero());

        // Decode the first packet to get the signal spec.
        let decoded = loop {
            let packet = probed
                .format
                .next_packet()
                .map_err(|e| anyhow::anyhow!("symphonia read error: {e}"))?;
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => break decoded,
                Err(Error::DecodeError(_)) => continue,
                Err(e) => anyhow::bail!("Opus decode error: {e}"),
            }
        };

        let spec = decoded.spec().to_owned();
        let buf = Self::sample_buffer(decoded, &spec);

        Ok(Self {
            decoder,
            format: probed.format,
            buffer: buf,
            buffer_offset: 0,
            spec,
            total_duration,
        })
    }

    fn sample_buffer(decoded: AudioBufferRef, spec: &SignalSpec) -> SymphoniaSampleBuffer<f32> {
        let duration = symphonia::core::units::Duration::from(decoded.capacity() as u64);
        let mut buffer = SymphoniaSampleBuffer::<f32>::new(duration, *spec);
        buffer.copy_interleaved_ref(decoded);
        buffer
    }

    fn decode_next(&mut self) -> Option<()> {
        loop {
            let packet = self.format.next_packet().ok()?;
            if packet.track_id() != self.format.tracks().first()?.id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    if decoded.frames() == 0 {
                        continue;
                    }
                    decoded.spec().clone_into(&mut self.spec);
                    self.buffer = Self::sample_buffer(decoded, &self.spec);
                    self.buffer_offset = 0;
                    return Some(());
                }
                Err(Error::DecodeError(_)) => continue,
                Err(_) => return None,
            }
        }
    }
}

impl Iterator for SymphoniaOpusSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.buffer_offset >= self.buffer.samples().len() {
            self.decode_next()?;
        }
        let sample = *self.buffer.samples().get(self.buffer_offset)?;
        self.buffer_offset += 1;
        Some(sample)
    }
}

impl Source for SymphoniaOpusSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len().saturating_sub(self.buffer_offset))
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(
            self.spec.channels.count() as u16,
        )
        .expect("at least one channel")
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(self.spec.rate).expect("non-zero sample rate")
    }

    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        // Use coarse seek via symphonia demuxer, then reset decoder.
        let seek_to = SeekTo::Time {
            time: pos.into(),
            track_id: None,
        };
        match self.format.seek(SeekMode::Coarse, seek_to) {
            Ok(_) => {
                self.decoder.reset();
                self.buffer_offset = usize::MAX; // force re-decode on next next()
                Ok(())
            }
            Err(_) => Err(SeekError::NotSupported {
                underlying_source: "SymphoniaOpusSource",
            }),
        }
    }
}

/// Unified source type: Symphonia's rodio Decoder for most formats,
/// custom symphonia-opus for Opus Ogg.
enum DecodedSource {
    Symphonia(Decoder<Cursor<Vec<u8>>>),
    Opus(SymphoniaOpusSource),
}

impl Iterator for DecodedSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        match self {
            Self::Symphonia(d) => d.next(),
            Self::Opus(d) => d.next(),
        }
    }
}

impl Source for DecodedSource {
    fn current_span_len(&self) -> Option<usize> {
        match self {
            Self::Symphonia(d) => d.current_span_len(),
            Self::Opus(d) => d.current_span_len(),
        }
    }

    fn channels(&self) -> rodio::ChannelCount {
        match self {
            Self::Symphonia(d) => d.channels(),
            Self::Opus(d) => d.channels(),
        }
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        match self {
            Self::Symphonia(d) => d.sample_rate(),
            Self::Opus(d) => d.sample_rate(),
        }
    }

    fn total_duration(&self) -> Option<Duration> {
        match self {
            Self::Symphonia(d) => d.total_duration(),
            Self::Opus(d) => d.total_duration(),
        }
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match self {
            Self::Symphonia(d) => d.try_seek(pos),
            Self::Opus(d) => d.try_seek(pos),
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
        let source = SymphoniaOpusSource::new(bytes)
            .context("decoding Opus audio with symphonia + libopus")?;
        return Ok(DecodedSource::Opus(source));
    }

    // Fall back to rodio::Decoder (Symphonia) for all other formats (MP3, FLAC, Vorbis, AAC, etc.).
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
