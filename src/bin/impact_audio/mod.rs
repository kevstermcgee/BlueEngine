//! Client-only sound: observe actual impacts, not input or wind-up animation.
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
pub struct ImpactAudio {
    sound: Option<Sound>,
    seen: u32,
    volume: f32,
    pub plays: u32,
}
impl ImpactAudio {
    pub async fn new() -> Self {
        Self::load(include_bytes!("../../../assets/audio/wrench-hit.wav"), 0.28).await
    }
    pub async fn pistol() -> Self {
        Self::load(
            include_bytes!("../../../assets/audio/pistol-shot.wav"),
            0.35,
        )
        .await
    }
    async fn load(bytes: &[u8], volume: f32) -> Self {
        let sound = load_sound_from_bytes(bytes).await.ok();
        Self {
            sound,
            volume,
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
                    volume: self.volume,
                },
            );
            self.plays += 1;
        }
    }
}
