//! Optional native frame evidence; never participates in simulation or rendering.
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Copy, Serialize)]
pub struct FrameSample {
    pub frame: u32,
    pub dt_ms: f64,
    pub gameplay: bool,
    pub preparing: bool,
    pub simulation_ms: f64,
    pub presentation_ms: f64,
    pub renderer_ms: f64,
    pub audio_update_ms: f64,
    pub save_ms: f64,
    pub cpu_ms: f64,
    pub sprite_cache_misses: u32,
}
pub struct FrameProfile {
    limit: Option<usize>,
    samples: Vec<FrameSample>,
}
fn distribution(mut values: Vec<f64>) -> Value {
    values.retain(|value| value.is_finite());
    if values.is_empty() {
        return Value::Null;
    }
    values.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        let index = ((values.len() - 1) as f64 * fraction).ceil() as usize;
        values[index]
    };
    json!({
        "count":values.len(),
        "mean":values.iter().sum::<f64>()/values.len() as f64,
        "p50":percentile(0.50),
        "p95":percentile(0.95),
        "p99":percentile(0.99),
        "max":values[values.len()-1]
    })
}
impl FrameProfile {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let limit = args
            .iter()
            .position(|arg| arg == "--profile-frames")
            .map(|index| {
                let value = args
                    .get(index + 1)
                    .ok_or("--profile-frames requires a positive frame count")?;
                let count: usize = value
                    .parse()
                    .map_err(|_| "--profile-frames requires a positive frame count")?;
                if count == 0 {
                    return Err("--profile-frames requires a positive frame count");
                }
                Ok(count)
            })
            .transpose()?;
        Ok(Self {
            limit,
            samples: Vec::new(),
        })
    }
    pub fn enabled(&self) -> bool {
        self.limit.is_some()
    }
    pub fn record(&mut self, sample: FrameSample) -> bool {
        let Some(limit) = self.limit else {
            return false;
        };
        let consecutive_gameplay = sample.gameplay
            && !sample.preparing
            && self
                .samples
                .last()
                .is_some_and(|last| last.gameplay && !last.preparing);
        if sample.cpu_ms > 1000. / 60.
            || consecutive_gameplay && sample.dt_ms > 1000. / 30.
            || sample.gameplay && !sample.preparing && sample.sprite_cache_misses > 0
        {
            println!(
                "{}",
                json!({"frame_profile_spike":sample,"consecutive_gameplay":consecutive_gameplay})
            );
        }
        self.samples.push(sample);
        self.samples.len() >= limit
    }
    pub fn summary(&self) -> Option<Value> {
        self.limit?;
        let active: Vec<_> = self
            .samples
            .iter()
            .filter(|sample| sample.gameplay && !sample.preparing)
            .collect();
        let intervals: Vec<_> = self
            .samples
            .windows(2)
            .filter(|pair| {
                pair.iter()
                    .all(|sample| sample.gameplay && !sample.preparing)
            })
            .map(|pair| pair[1].dt_ms)
            .collect();
        Some(json!({
            "frame_profile":true,
            "frames":self.samples.len(),
            "active_frames":active.len(),
            "preparation_frames":self.samples.iter().filter(|sample| sample.preparing).count(),
            "active_cpu_ms":distribution(active.iter().map(|sample| sample.cpu_ms).collect()),
            "active_frame_intervals_ms":distribution(intervals),
            "active_simulation_ms":distribution(active.iter().map(|sample| sample.simulation_ms).collect()),
            "active_presentation_ms":distribution(active.iter().map(|sample| sample.presentation_ms).collect()),
            "active_renderer_ms":distribution(active.iter().map(|sample| sample.renderer_ms).collect()),
            "active_audio_update_ms":distribution(active.iter().map(|sample| sample.audio_update_ms).collect()),
            "active_save_ms":distribution(active.iter().map(|sample| sample.save_ms).collect()),
            "active_sprite_cache_misses":active.iter().map(|sample| sample.sprite_cache_misses).sum::<u32>(),
            "active_cpu_over_budget":active.iter().filter(|sample| sample.cpu_ms>1000./60.).count(),
            "all_cpu_ms":distribution(self.samples.iter().map(|sample| sample.cpu_ms).collect()),
            "scope":"Observed native process timing. Preparation is separate; intervals count only consecutive active frames. Does not certify physical display latency or controller hardware."
        }))
    }
}
