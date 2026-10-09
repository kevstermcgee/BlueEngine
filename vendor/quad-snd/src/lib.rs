//! Loading and playing sounds.

#![allow(warnings)]

mod error;

pub use error::Error;

#[cfg(target_os = "android")]
#[path = "opensles_snd.rs"]
mod snd;

#[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd"))]
#[path = "alsa_snd.rs"]
mod snd;

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "coreaudio_snd.rs"]
mod snd;

#[cfg(target_os = "windows")]
#[path = "wasapi_snd.rs"]
mod snd;

#[cfg(target_arch = "wasm32")]
#[path = "web_snd.rs"]
mod snd;

#[cfg(not(target_arch = "wasm32"))]
mod mixer;

pub use snd::{AudioContext, Playback, Sound};

pub struct PlaySoundParams {
    pub looped: bool,
    pub volume: f32,
}

impl Default for PlaySoundParams {
    fn default() -> PlaySoundParams {
        PlaySoundParams {
            looped: false,
            volume: 1.,
        }
    }
}

/// Health of the native playback worker, separate from decoded/resource evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendState {
    NotStarted,
    Starting,
    Ready,
    Unavailable(String),
}
static BACKEND: std::sync::Mutex<BackendState> = std::sync::Mutex::new(BackendState::NotStarted);
pub fn backend_state() -> BackendState {
    BACKEND
        .lock()
        .map(|s| s.clone())
        .unwrap_or_else(|_| BackendState::Unavailable("audio status lock failed".into()))
}
pub(crate) fn state(value: BackendState) {
    if let Ok(mut current) = BACKEND.lock() {
        if matches!(*current, BackendState::Unavailable(_)) {
            return;
        }
        if let BackendState::Unavailable(error) = &value {
            eprintln!("AUDIO-BACKEND: {error}; gameplay continues silently; validate resources separately and check the output device");
        }
        *current = value;
    }
}
