//! Client-only sound: observe actual impacts, not input or wind-up animation.
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
pub struct ImpactAudio {
    sound: Option<Sound>,
    seen: u32,
    pub plays: u32,
}
impl ImpactAudio {
    pub async fn new() -> Self {
        let sound = load_sound_from_bytes(include_bytes!("../../../assets/audio/wrench-hit.wav"))
            .await
            .ok();
        Self {
            sound,
            seen: 0,
            plays: 0,
        }
    }
    pub fn update(&mut self, hits: u32) {
        if self.seen == hits {
            return;
        }
        self.seen = hits;
        if let Some(sound) = &self.sound {
            play_sound(
                sound,
                PlaySoundParams {
                    looped: false,
                    volume: 0.28,
                },
            );
            self.plays += 1;
        }
    }
}
