//! Stock GameDocument HUD wording, counter labels/units and palette; reads authoritative state only.
use super::game::{GameDocument, GameState};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StockPresentation {
    pub objective: Option<String>,
    pub success: Option<String>,
    pub failure: Option<String>,
    pub counters: BTreeMap<String, CounterDisplay>,
    pub palette: Palette,
    pub hud: Hud,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CounterDisplay {
    pub visible: bool,
    pub label: Option<String>,
    pub units: String,
    pub format: CounterFormat,
}
impl Default for CounterDisplay {
    fn default() -> Self {
        Self {
            visible: true,
            label: None,
            units: String::new(),
            format: CounterFormat::Number,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterFormat {
    #[default]
    Number,
    /// The authoritative integer is seconds, displayed as minutes:seconds. Negative values retain their sign.
    Clock,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Palette {
    pub background: [f32; 4],
    pub panel: [f32; 4],
    pub text: [f32; 4],
    pub accent: [f32; 4],
    pub success: [f32; 4],
    pub failure: [f32; 4],
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            background: [0.48, 0.67, 0.8, 1.],
            panel: [0.04, 0.08, 0.1, 0.85],
            text: [1.; 4],
            accent: [1.; 4],
            success: [1.; 4],
            failure: [1.; 4],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hud {
    pub scale: f32,
    pub margin: f32,
    pub width: Option<f32>,
    pub crosshair: bool,
}
impl Default for Hud {
    fn default() -> Self {
        Self {
            scale: 1.,
            margin: 12.,
            width: None,
            crosshair: true,
        }
    }
}

impl StockPresentation {
    pub fn validate(&self, counters: &BTreeMap<String, i32>) -> Result<()> {
        let text_ok = |text: &str, limit: usize| {
            !text.is_empty() && text.len() <= limit && text.bytes().all(|b| (32..=126).contains(&b))
        };
        if [&self.objective, &self.success, &self.failure]
            .into_iter()
            .flatten()
            .any(|s| !text_ok(s, 200))
        {
            return Err("Presentation wording must be 1..200 printable ASCII bytes".into());
        }
        for (name, display) in &self.counters {
            if !counters.contains_key(name)
                || display.label.as_ref().is_some_and(|s| !text_ok(s, 48))
                || (!display.units.is_empty() && !text_ok(&display.units, 12))
            {
                return Err(format!("Invalid presentation counter {name}: use a declared counter, label <=48, units <=12").into());
            }
        }
        if [
            &self.palette.background,
            &self.palette.panel,
            &self.palette.text,
            &self.palette.accent,
            &self.palette.success,
            &self.palette.failure,
        ]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite() || !(0.0..=1.).contains(v))
        {
            return Err("Presentation palette components must be finite RGBA in 0..1".into());
        }
        if !self.hud.scale.is_finite()
            || !(0.75..=1.5).contains(&self.hud.scale)
            || !self.hud.margin.is_finite()
            || !(8.0..=48.).contains(&self.hud.margin)
            || self
                .hud
                .width
                .is_some_and(|w| !w.is_finite() || !(240.0..=900.).contains(&w))
        {
            return Err(
                "HUD requires scale 0.75..1.5, margin 8..48 and optional width 240..900".into(),
            );
        }
        Ok(())
    }

    /// Formatting only: winning, losing and counter values all come from GameRuntime's authoritative state.
    pub fn status(&self, document: &GameDocument, state: &GameState) -> String {
        if state.completed {
            return self
                .success
                .as_deref()
                .unwrap_or("Objective complete! E / X / R to play again")
                .into();
        }
        if state.failed {
            return self
                .failure
                .as_deref()
                .unwrap_or("Objective failed! E / X / R to try again")
                .into();
        }
        document
            .counters
            .keys()
            .zip(&state.counters)
            .filter_map(|(name, value)| {
                let default = CounterDisplay::default();
                let display = self.counters.get(name).unwrap_or(&default);
                if !display.visible {
                    return None;
                }
                let number = match display.format {
                    CounterFormat::Number => value.to_string(),
                    CounterFormat::Clock => format!(
                        "{}{}:{:02}",
                        if *value < 0 { "-" } else { "" },
                        value.unsigned_abs() / 60,
                        value.unsigned_abs() % 60
                    ),
                };
                Some(format!(
                    "{}: {}{}{}",
                    display.label.as_deref().unwrap_or(name),
                    number,
                    if display.units.is_empty() { "" } else { " " },
                    display.units
                ))
            })
            .collect::<Vec<_>>()
            .join("   ")
    }
}
