// https://github.com/floooh/sokol/blob/master/sokol_audio.h
// https://github.com/norse-rs/audir/blob/master/audir/src/wasapi/mod.rs

use crate::PlaySoundParams;

pub use crate::mixer::Playback;

use winapi::shared::guiddef::{CLSID, IID};
use winapi::shared::ksmedia;
use winapi::shared::minwindef::*;
use winapi::shared::mmreg::*;
use winapi::um::audioclient::*;
use winapi::um::audiosessiontypes::*;
use winapi::um::combaseapi::*;
use winapi::um::handleapi::CloseHandle;
use winapi::um::mmdeviceapi::*;
use winapi::um::objbase::*;
use winapi::um::synchapi::*;
use winapi::um::winbase::*;

use std::sync::mpsc;

// thanks sokol_audio!
// https://github.com/floooh/sokol/blob/master/sokol_audio.h#L559
static IID_IAudioClient: IID = IID {
    Data1: 0x1cb9ad4c,
    Data2: 0xdbfa,
    Data3: 0x4c32,
    Data4: [0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2],
};
static IID_IMMDeviceEnumerator: IID = IID {
    Data1: 0xa95664d2,
    Data2: 0x9614,
    Data3: 0x4f35,
    Data4: [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
};
static CLSID_IMMDeviceEnumerator: CLSID = CLSID {
    Data1: 0xbcde0395,
    Data2: 0xe52f,
    Data3: 0x467c,
    Data4: [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
};
static IID_IAudioRenderClient: IID = IID {
    Data1: 0xf294acfc,
    Data2: 0x3146,
    Data3: 0x4483,
    Data4: [0xa7, 0xbf, 0xad, 0xdc, 0xa7, 0xc2, 0x60, 0xe2],
};
static IID_Devinterface_Audio_Render: IID = IID {
    Data1: 0xe6327cad,
    Data2: 0xdcec,
    Data3: 0x4949,
    Data4: [0xae, 0x8a, 0x99, 0x1e, 0x97, 0x6a, 0x79, 0xd2],
};
static IID_IActivateAudioInterface_Completion_Handler: IID = IID {
    Data1: 0x94ea2b94,
    Data2: 0xe9cc,
    Data3: 0x49e0,
    Data4: [0xc0, 0xff, 0xee, 0x64, 0xca, 0x8f, 0x5b, 0x90],
};

mod consts {
    pub const CHANNELS: u32 = 2;
    pub const SAMPLE_RATE: u32 = 44100;
    pub const BUFFER_FRAMES: u32 = 4096;
}

struct Com(*mut winapi::um::unknwnbase::IUnknown);
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                (*self.0).Release();
            }
        }
    }
}
struct Event(winapi::shared::ntdef::HANDLE);
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct ComInit;
impl Drop for ComInit {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
fn checked(hr: i32, operation: &str) -> Result<(), String> {
    if hr < 0 {
        Err(format!("WASAPI {operation} failed (0x{:08x})", hr as u32))
    } else {
        Ok(())
    }
}
unsafe fn audio_thread(mut mixer: crate::mixer::Mixer) -> Result<(), String> {
    checked(
        CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED),
        "COM initialization",
    )?;
    let _com = ComInit;

    let buffer_end_event = CreateEventA(std::ptr::null_mut(), FALSE, FALSE, std::ptr::null());
    if buffer_end_event.is_null() {
        return Err("WASAPI event creation failed".into());
    }
    let _event = Event(buffer_end_event);

    let mut device_enumerator: *mut IMMDeviceEnumerator = std::ptr::null_mut();
    let hr = CoCreateInstance(
        &CLSID_IMMDeviceEnumerator,
        std::ptr::null_mut(),
        CLSCTX_ALL,
        &IID_IMMDeviceEnumerator,
        &mut device_enumerator as *mut _ as _,
    );
    checked(hr, "CoCreatInstance failed")?;
    let _device_enumerator = Com(device_enumerator as _);

    let mut device: *mut IMMDevice = std::ptr::null_mut();
    let hr = (*device_enumerator).GetDefaultAudioEndpoint(eRender, eConsole, &mut device);
    checked(hr, "GetDefaultAudioEndPoint failed")?;
    let _device = Com(device as _);

    let mut audio_client: *mut IAudioClient = std::ptr::null_mut();
    let hr = (*device).Activate(
        &IID_IAudioClient,
        CLSCTX_ALL,
        std::ptr::null_mut(),
        &mut audio_client as *mut _ as _,
    );
    checked(hr, "Device Activate failed")?;
    let _audio_client = Com(audio_client as _);

    let mut state = 0;
    checked((*device).GetState(&mut state), "GetState failed")?;
    if state & DEVICE_STATE_ACTIVE == 0 {
        return Err("WASAPI default device is not active".into());
    }

    let format = WAVEFORMATEX {
        nChannels: consts::CHANNELS as _,
        nSamplesPerSec: consts::SAMPLE_RATE as _,
        wFormatTag: WAVE_FORMAT_EXTENSIBLE,
        wBitsPerSample: 32,
        nBlockAlign: consts::CHANNELS as u16 * 4,
        nAvgBytesPerSec: consts::CHANNELS as u32 * consts::SAMPLE_RATE as u32 * 4,
        cbSize: (std::mem::size_of::<WAVEFORMATEXTENSIBLE>() - std::mem::size_of::<WAVEFORMATEX>())
            as _,
    };

    const FRONT_LEFT: u32 = 0b0001;
    const FRONT_RIGHT: u32 = 0b0010;

    let format_extensible = WAVEFORMATEXTENSIBLE {
        Format: format,
        Samples: 4 * 8,
        dwChannelMask: FRONT_LEFT | FRONT_RIGHT,
        SubFormat: ksmedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    };

    // https://docs.microsoft.com/en-us/windows/win32/coreaudio/audclnt-streamflags-xxx-constants
    const AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM: u32 = 0x80000000;
    const AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY: u32 = 0x08000000;

    let dur = consts::BUFFER_FRAMES as f64 / (consts::SAMPLE_RATE as f64 * 1.0 / 10000000.0);
    let hr = (*audio_client).Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK
            | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        dur as _,
        0,
        &format_extensible as *const _ as _,
        std::ptr::null(),
    );
    if hr < 0 {
        println!("Error code: 0x{:x}", hr as u32);
    }
    checked(hr, "audio_client.Initialize failed")?;

    let mut dst_buffer_frames = 0;
    let hr = (*audio_client).GetBufferSize(&mut dst_buffer_frames);
    checked(hr, "GetBufferSize failed")?;

    let mut render_client: *mut IAudioRenderClient = std::ptr::null_mut();
    let hr = (*audio_client).GetService(&IID_IAudioRenderClient, &mut render_client as *mut _ as _);
    checked(hr, "audio client GetService")?;
    let _render_client = Com(render_client as _);

    let hr = (*audio_client).SetEventHandle(buffer_end_event);
    checked(hr, "SetEventHandle failed")?;

    checked((*audio_client).Start(), "start playback")?;
    crate::state(crate::BackendState::Ready);
    loop {
        if WaitForSingleObject(buffer_end_event, 1000) == WAIT_FAILED {
            return Err("WASAPI wait failed".into());
        }

        let mut padding = 0;
        checked(
            (*audio_client).GetCurrentPadding(&mut padding),
            "current padding",
        )?;
        let num_frames = dst_buffer_frames
            .checked_sub(padding)
            .ok_or("WASAPI invalid padding")?;

        if num_frames == 0 {
            continue;
        }

        let mut wasapi_buffer: *mut u8 = std::ptr::null_mut();
        checked(
            (*render_client).GetBuffer(num_frames, &mut wasapi_buffer),
            "playback buffer",
        )?;
        if wasapi_buffer.is_null() {
            return Err("WASAPI returned a null buffer".into());
        }

        let buffer = std::slice::from_raw_parts_mut(
            wasapi_buffer as *mut f32,
            num_frames as usize * consts::CHANNELS as usize,
        );

        mixer.fill_audio_buffer(buffer, num_frames as _);

        checked(
            (*render_client).ReleaseBuffer(num_frames, 0),
            "release playback buffer",
        )?;
        if !mixer.connected() {
            checked((*audio_client).Stop(), "stop playback")?;
            return Ok(());
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
