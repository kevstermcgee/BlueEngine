// roughly based on http://equalarea.com/paul/alsa-audio.html

use crate::{error::Error, PlaySoundParams};

use quad_alsa_sys as sys;

use std::sync::mpsc;

pub use crate::mixer::Playback;

mod consts {
    pub const DEVICES: &[&str] = &["default\0", "pipewire\0"];
    pub const RATE: u32 = 44100;
    pub const CHANNELS: u32 = 2;
    pub const PCM_BUFFER_SIZE: ::std::os::raw::c_ulong = 4096;
}

struct Pcm(*mut sys::snd_pcm_t);
impl Drop for Pcm {
    fn drop(&mut self) {
        unsafe {
            sys::snd_pcm_close(self.0);
        }
    }
}
struct Hw(*mut sys::snd_pcm_hw_params_t);
impl Drop for Hw {
    fn drop(&mut self) {
        unsafe {
            sys::snd_pcm_hw_params_free(self.0);
        }
    }
}
struct Sw(*mut sys::snd_pcm_sw_params_t);
impl Drop for Sw {
    fn drop(&mut self) {
        unsafe {
            sys::snd_pcm_sw_params_free(self.0);
        }
    }
}
fn checked(code: i32, operation: &str) -> Result<(), String> {
    if code < 0 {
        Err(format!("ALSA {operation} failed (code {code})"))
    } else {
        Ok(())
    }
}
unsafe fn setup_pcm_device() -> Result<Pcm, String> {
    let mut handle = std::ptr::null_mut();
    if !consts::DEVICES.iter().any(|device| {
        sys::snd_pcm_open(
            &mut handle,
            device.as_ptr() as _,
            sys::SND_PCM_STREAM_PLAYBACK,
            0,
        ) >= 0
    }) {
        return Err("ALSA could not open default or pipewire playback device".into());
    }
    let pcm = Pcm(handle);
    let mut hw = std::ptr::null_mut();
    checked(
        sys::snd_pcm_hw_params_malloc(&mut hw),
        "allocate hardware parameters",
    )?;
    let hw = Hw(hw);
    checked(
        sys::snd_pcm_hw_params_any(handle, hw.0),
        "read hardware parameters",
    )?;
    checked(
        sys::snd_pcm_hw_params_set_access(handle, hw.0, sys::SND_PCM_ACCESS_RW_INTERLEAVED),
        "interleaved access",
    )?;
    checked(
        sys::snd_pcm_hw_params_set_format(handle, hw.0, sys::SND_PCM_FORMAT_FLOAT_LE),
        "float PCM format",
    )?;
    checked(
        sys::snd_pcm_hw_params_set_buffer_size(handle, hw.0, consts::PCM_BUFFER_SIZE),
        "buffer size",
    )?;
    checked(
        sys::snd_pcm_hw_params_set_channels(handle, hw.0, consts::CHANNELS),
        "stereo channels",
    )?;
    let mut rate = consts::RATE;
    checked(
        sys::snd_pcm_hw_params_set_rate_near(handle, hw.0, &mut rate, std::ptr::null_mut()),
        "44100 Hz sample rate",
    )?;
    if rate != consts::RATE {
        return Err(format!(
            "ALSA selected {rate} Hz; this mixer requires 44100 Hz"
        ));
    }
    checked(
        sys::snd_pcm_hw_params(handle, hw.0),
        "apply hardware parameters",
    )?;
    let mut sw = std::ptr::null_mut();
    checked(
        sys::snd_pcm_sw_params_malloc(&mut sw),
        "allocate software parameters",
    )?;
    let sw = Sw(sw);
    checked(
        sys::snd_pcm_sw_params_current(handle, sw.0),
        "read software parameters",
    )?;
    checked(
        sys::snd_pcm_sw_params_set_start_threshold(handle, sw.0, 0),
        "start threshold",
    )?;
    checked(
        sys::snd_pcm_sw_params(handle, sw.0),
        "apply software parameters",
    )?;
    checked(sys::snd_pcm_prepare(handle), "prepare playback")?;
    Ok(pcm)
}
unsafe fn audio_thread(mut mixer: crate::mixer::Mixer) -> Result<(), String> {
    let pcm = setup_pcm_device()?;
    crate::state(crate::BackendState::Ready);
    let mut buffer = vec![0.0; consts::PCM_BUFFER_SIZE as usize * 2];
    let period =
        std::time::Duration::from_secs_f64(consts::PCM_BUFFER_SIZE as f64 / consts::RATE as f64);
    loop {
        let start = std::time::Instant::now();
        let ready = sys::snd_pcm_wait(pcm.0, 1000);
        if ready < 0 {
            checked(sys::snd_pcm_recover(pcm.0, ready, 0), "recover wait")?;
            continue;
        }
        if ready == 0 {
            return Err("ALSA playback device timed out (1000 ms)".into());
        }
        mixer.fill_audio_buffer(&mut buffer, consts::PCM_BUFFER_SIZE as usize);
        if !mixer.connected() {
            return Ok(());
        }
        let mut offset = 0;
        while offset < consts::PCM_BUFFER_SIZE as usize {
            let written = sys::snd_pcm_writei(
                pcm.0,
                buffer[offset * 2..].as_ptr() as _,
                (consts::PCM_BUFFER_SIZE as usize - offset) as _,
            );
            if written < 0 {
                checked(
                    sys::snd_pcm_recover(pcm.0, written as _, 0),
                    "recover write",
                )?;
            } else if written == 0 {
                return Err("ALSA write made no progress".into());
            } else {
                offset += written as usize;
            }
        }
        // Real devices normally pace writes. Null/software devices can accept instantly;
        // do not consume a CPU core or advance the mixer faster than its sample clock.
        if let Some(remaining) = period.checked_sub(start.elapsed()) {
            std::thread::sleep(remaining);
        }
    }
}

pub struct AudioContext {
    pub(crate) mixer_ctrl: crate::mixer::MixerControl,
}

impl AudioContext {
    pub fn new() -> AudioContext {
        use crate::mixer::Mixer;

        let (mixer_builder, mixer_ctrl) = Mixer::new();
        crate::state(crate::BackendState::Starting);
        let worker = std::thread::Builder::new()
            .name("blueengine-audio".into())
            .spawn(move || unsafe {
                if let Err(error) = audio_thread(mixer_builder.build()) {
                    crate::state(crate::BackendState::Unavailable(error));
                }
            });
        if let Err(error) = worker {
            crate::state(crate::BackendState::Unavailable(format!(
                "Audio worker could not start: {error}"
            )));
        }

        AudioContext { mixer_ctrl }
    }
}

pub struct Sound {
    sound_id: u32,
}

impl Sound {
    pub fn from_samples(ctx: &AudioContext, samples: Vec<f32>) -> Sound {
        Sound {
            sound_id: ctx.mixer_ctrl.load_samples(samples),
        }
    }
    pub fn load(ctx: &AudioContext, data: &[u8]) -> Sound {
        let sound_id = ctx.mixer_ctrl.load(data);

        Sound { sound_id }
    }

    pub fn play(&self, ctx: &AudioContext, params: PlaySoundParams) -> Playback {
        ctx.mixer_ctrl.play(self.sound_id, params)
    }

    pub fn stop(&self, ctx: &AudioContext) {
        ctx.mixer_ctrl.stop_all(self.sound_id);
    }

    pub fn set_volume(&self, ctx: &AudioContext, volume: f32) {
        ctx.mixer_ctrl.set_volume_all(self.sound_id, volume);
    }

    pub fn delete(&self, ctx: &AudioContext) {
        ctx.mixer_ctrl.delete(self.sound_id);
    }
}
