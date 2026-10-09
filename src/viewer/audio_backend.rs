//! Native playback boundary. Resources and authoritative simulation do not require a device.
pub use quad_snd::{BackendState, PlaySoundParams};
use std::{cell::RefCell, rc::Rc};
thread_local! { static CONTEXT: RefCell<Option<quad_snd::AudioContext>> = const { RefCell::new(None) }; }
fn with_context<R>(f: impl FnOnce(&quad_snd::AudioContext) -> R) -> R {
    CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        f(ctx.get_or_insert_with(quad_snd::AudioContext::new))
    })
}
pub fn state() -> BackendState {
    quad_snd::backend_state()
}
struct Guard(quad_snd::Sound);
impl Drop for Guard {
    fn drop(&mut self) {
        CONTEXT.with(|ctx| {
            if let Some(ctx) = ctx.borrow().as_ref() {
                self.0.delete(ctx);
            }
        });
    }
}
#[derive(Clone)]
pub struct Sound(Rc<Guard>);
pub async fn load_sound_from_bytes(bytes: &[u8]) -> Result<Sound, String> {
    // The engine submits checked PCM WAV only; invalid resources remain visible even without hardware.
    let (channels, samples) = crate::viewer::devkit::audio_project::checked_wav(bytes)?;
    let stereo = if channels == 1 {
        samples.into_iter().flat_map(|s| [s, s]).collect()
    } else {
        samples
    };
    Ok(Sound(Rc::new(Guard(with_context(|ctx| {
        quad_snd::Sound::from_samples(ctx, stereo)
    })))))
}
pub fn play_sound(sound: &Sound, params: PlaySoundParams) {
    if matches!(state(), BackendState::Ready) {
        with_context(|ctx| {
            sound.0 .0.play(ctx, params);
        });
    }
}
pub fn set_sound_volume(sound: &Sound, volume: f32) {
    if matches!(state(), BackendState::Ready) {
        with_context(|ctx| sound.0 .0.set_volume(ctx, volume));
    }
}
pub fn stop_sound(sound: &Sound) {
    if matches!(state(), BackendState::Ready) {
        with_context(|ctx| sound.0 .0.stop(ctx));
    }
}
