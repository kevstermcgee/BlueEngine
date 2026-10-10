//! Shared presentation audio, using the existing checked AudioBank runtime.
use super::{AudioBankSpec, AudioBinding, GameLogic};
use crate::viewer::kit::audio::{AudioBank, AudioState, Rendered, SoundBank};
use std::collections::{BTreeMap, BTreeSet};

struct Bank {
    spec: &'static AudioBankSpec,
    layers: Vec<String>,
    audio: AudioBank,
}
pub(super) struct Audio {
    banks: Vec<Bank>,
    defaults: SoundBank,
    muted: bool,
    submitted: BTreeMap<String, u64>,
}

fn validate_bindings(
    bindings: &[AudioBinding],
    names: &BTreeMap<&str, BTreeSet<String>>,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for binding in bindings {
        if binding.event.is_empty()
            || !binding.volume.is_finite()
            || !(0. ..=1.).contains(&binding.volume)
        {
            return Err("Audio binding needs a nonempty event and finite volume 0..1".into());
        }
        if !seen.insert((binding.event, binding.bank, binding.cue)) {
            return Err(format!(
                "Duplicate audio binding {} / {} / {}",
                binding.event, binding.bank, binding.cue
            ));
        }
        if !names
            .get(binding.bank)
            .is_some_and(|cues| cues.contains(binding.cue))
        {
            return Err(format!(
                "Audio event {} refers to missing effect {}/{}",
                binding.event, binding.bank, binding.cue
            ));
        }
    }
    Ok(())
}

impl Audio {
    pub async fn new<G: GameLogic>(muted: bool) -> Result<Self, String> {
        let mut banks = Vec::new();
        let mut names = BTreeMap::new();
        if !muted {
            for spec in G::audio_banks() {
                let metadata = crate::runtime::audio_project::AudioBundle::load(
                    std::path::Path::new(spec.root),
                )?;
                if names
                    .insert(spec.id, metadata.effects.keys().cloned().collect())
                    .is_some()
                {
                    return Err(format!("Duplicate audio bank ID {}", spec.id));
                }
                let audio = AudioBank::load(spec.root, false, 1., 1.).await?;
                banks.push(Bank {
                    spec,
                    layers: metadata.music.keys().cloned().collect(),
                    audio,
                });
            }
            validate_bindings(G::audio_bindings(), &names)?;
        }
        let defaults = SoundBank::start(muted || !G::default_sounds(), 0.3, 0., || {
            use crate::runtime::synth::{self, Preset};
            Rendered {
                sfx: [Preset::Coin, Preset::Hit, Preset::Success]
                    .into_iter()
                    .map(|preset| vec![synth::wav_bytes(&synth::render(preset, 0, 7), synth::RATE)])
                    .collect(),
                stems: vec![],
            }
        })
        .await;
        let mut result = Self {
            banks,
            defaults,
            muted,
            submitted: BTreeMap::new(),
        };
        loop {
            result.defaults.poll().await;
            for bank in &mut result.banks {
                bank.audio.sounds.poll().await;
                if bank.audio.sounds.state() == AudioState::Failed {
                    return Err(format!(
                        "Audio bank {} failed: {:?}",
                        bank.spec.id,
                        bank.audio.sounds.errors()
                    ));
                }
            }
            if result.defaults.state() == AudioState::Failed {
                return Err(format!(
                    "Default audio failed: {:?}",
                    result.defaults.errors()
                ));
            }
            if result.defaults.ready() && result.banks.iter().all(|b| b.audio.sounds.ready()) {
                break;
            }
            macroquad::prelude::next_frame().await;
        }
        Ok(result)
    }

    pub fn event<G: GameLogic>(
        &mut self,
        event: &str,
        fallback: Option<usize>,
        enabled: bool,
    ) -> Result<(), String> {
        let bindings: Vec<_> = G::audio_bindings()
            .iter()
            .filter(|b| b.event == event)
            .collect();
        if !self.muted && enabled {
            for binding in &bindings {
                let bank = self
                    .banks
                    .iter_mut()
                    .find(|b| b.spec.id == binding.bank)
                    .expect("validated binding");
                bank.audio.sounds.sfx_volume = 1.;
                bank.audio.play(binding.cue, binding.volume)?;
                *self
                    .submitted
                    .entry(format!("{event}:{}/{}", binding.bank, binding.cue))
                    .or_default() += 1;
            }
            if bindings.is_empty() && G::default_sounds() {
                if let Some(cue) = fallback.filter(|i| *i < 3) {
                    self.defaults.play(cue, 1.);
                    *self
                        .submitted
                        .entry(format!("{event}:default/{cue}"))
                        .or_default() += 1;
                }
            }
        }
        Ok(())
    }

    pub fn update<G: GameLogic>(
        &mut self,
        game: &G,
        active: bool,
        sound: bool,
        music: bool,
        dt: f32,
    ) -> Result<(), String> {
        for bank in &mut self.banks {
            bank.audio.sounds.sfx_volume = if sound { 1. } else { 0. };
            bank.audio.sounds.music_volume = if if bank.spec.music { music } else { sound } {
                1.
            } else {
                0.
            };
            let levels: Vec<_> = bank
                .layers
                .iter()
                .map(|layer| (layer.as_str(), game.audio_level(bank.spec.id, layer)))
                .collect();
            if active {
                bank.audio.music(dt, &levels)?;
            } else {
                bank.audio.sounds.stop_music();
            }
        }
        Ok(())
    }

    pub fn evidence(&self) -> serde_json::Value {
        serde_json::json!({"muted":self.muted, "defaults":self.defaults.status(),
            "banks":self.banks.iter().map(|b| (b.spec.id, b.audio.sounds.status())).collect::<BTreeMap<_,_>>(),
            "submitted":self.submitted, "physical_audibility_verified":false})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_bindings_reject_typos_invalid_volumes_and_duplicates() {
        let names = BTreeMap::from([("paper", BTreeSet::from(["pencil".into()]))]);
        let valid = || AudioBinding {
            event: "select",
            bank: "paper",
            cue: "pencil",
            volume: 0.4,
        };
        assert!(validate_bindings(&[valid()], &names).is_ok());
        for bad in [
            AudioBinding {
                cue: "absent",
                ..valid()
            },
            AudioBinding {
                bank: "absent",
                ..valid()
            },
            AudioBinding {
                volume: f32::NAN,
                ..valid()
            },
            AudioBinding {
                volume: 1.1,
                ..valid()
            },
            AudioBinding {
                event: "",
                ..valid()
            },
        ] {
            assert!(validate_bindings(&[bad], &names).is_err());
        }
        assert!(validate_bindings(&[valid(), valid()], &names).is_err());
    }
}
