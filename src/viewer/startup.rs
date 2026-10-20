//! Shared launch presentation. Run before constructing a simulation or starting local authority.
//! Explicit capture/script/verification runs keep their original frame numbers and window sizes.
use macroquad::prelude::*;

const DURATION: f32 = 1.9;
static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether this is an explicitly automated run. Ordinary play always shows the studio card.
pub fn unattended(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "--script"
            || arg == "--playback"
            || arg == "--scenario"
            || arg == "--verify"
            || arg == "--probe"
            || arg == "--frames"
            || arg == "--exit-after"
            || arg.starts_with("--capture")
    })
}

/// Fullscreen for normal play. A requested size, windowed mode or automated run retains a window.
/// `--fullscreen` can explicitly request fullscreen with `--size`; `--windowed` takes precedence.
pub fn fullscreen_for(args: &[String]) -> bool {
    !args.iter().any(|arg| arg == "--windowed")
        && !unattended(args)
        && (!args.iter().any(|arg| arg == "--size") || args.iter().any(|arg| arg == "--fullscreen"))
}

/// Initial fullscreen state shared by window configuration and the F/F11 toggle state.
pub fn default_fullscreen() -> bool {
    fullscreen_for(&std::env::args().collect::<Vec<_>>())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Display,
    Release,
    Finished,
}

#[derive(Default)]
struct Intro {
    elapsed: f32,
    finishing: bool,
}
impl Intro {
    fn advance(&mut self, seconds: f32, skip: bool, held: bool) -> Stage {
        if seconds.is_finite() {
            self.elapsed += seconds.clamp(0., 0.1);
        }
        self.finishing |= self.elapsed >= DURATION || (self.elapsed >= 0.15 && skip);
        if !self.finishing {
            Stage::Display
        } else if held {
            Stage::Release
        } else {
            Stage::Finished
        }
    }
}

/// Brief, silent, text-only studio card. The game's own title/menu follows.
/// No simulation, game clock, gameplay audio or input accumulator exists during this future.
/// Keyboard, mouse and controller can skip; their held state is released before play receives input.
pub async fn show() {
    let args = std::env::args().collect::<Vec<_>>();
    if unattended(&args) || SHOWN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    super::game_text::initialize();
    let mut intro = Intro::default();
    #[cfg(feature = "gamepad")]
    let mut pads = super::gamepad::Gamepads::new().ok();
    loop {
        #[allow(unused_mut)]
        let mut skip = !get_keys_pressed().is_empty()
            || is_mouse_button_pressed(MouseButton::Left)
            || is_mouse_button_pressed(MouseButton::Right);
        #[allow(unused_mut)]
        let mut held = !get_keys_down().is_empty()
            || is_mouse_button_down(MouseButton::Left)
            || is_mouse_button_down(MouseButton::Right)
            || is_mouse_button_down(MouseButton::Middle);
        #[cfg(feature = "gamepad")]
        if let Some(pads) = pads.as_mut() {
            use super::gamepad::Button;
            let pad = pads.poll(true);
            skip |= pad.menu_select() || pad.menu_back() || pad.pressed(Button::Start);
            held |= [
                Button::South,
                Button::East,
                Button::North,
                Button::West,
                Button::Start,
                Button::Select,
                Button::LeftTrigger,
                Button::RightTrigger,
                Button::LeftTrigger2,
                Button::RightTrigger2,
                Button::LeftThumb,
                Button::RightThumb,
                Button::DPadUp,
                Button::DPadDown,
                Button::DPadLeft,
                Button::DPadRight,
            ]
            .iter()
            .any(|button| pad.down(*button))
                || pad
                    .left_stick
                    .iter()
                    .chain(&pad.right_stick)
                    .any(|axis| axis.abs() > 0.1)
                || pad.triggers.iter().any(|trigger| *trigger > 0.1);
        }
        let stage = intro.advance(get_frame_time(), skip, held);
        draw(intro.elapsed, stage == Stage::Release);
        // Consume this frame's press edges, including an immediate press/release, before returning.
        next_frame().await;
        if stage == Stage::Finished {
            break;
        }
    }
}

fn draw(elapsed: f32, releasing: bool) {
    set_default_camera();
    clear_background(Color::new(0.018, 0.031, 0.060, 1.));
    let (w, h) = (screen_width(), screen_height());
    let scale = (w / 900.).min(h / 600.).clamp(0.3, 2.);
    let fade = (elapsed / 0.24).clamp(0., 1.);
    let center_y = h * 0.5;
    let title_size = 42. * scale;
    let text = "BlueEngine Games";
    let width = super::game_text::measure_text(text, None, 42, scale).width;
    super::game_text::draw_text(
        text,
        (w - width) / 2.,
        center_y,
        title_size,
        Color::new(0.94, 0.97, 1., fade),
    );
    let line_w = 148. * scale;
    draw_rectangle(
        (w - line_w) / 2.,
        center_y + 28. * scale,
        line_w,
        2. * scale,
        Color::new(0.22, 0.58, 0.94, fade * 0.6),
    );
    let hint = if releasing {
        "Release controls to continue"
    } else {
        "Press any key, click, or A to skip"
    };
    let width = super::game_text::measure_text(hint, None, 15, scale).width;
    super::game_text::draw_text(
        hint,
        (w - width) / 2.,
        h - 42. * scale,
        15. * scale,
        Color::new(0.52, 0.62, 0.76, fade),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(flags: &[&str]) -> Vec<String> {
        std::iter::once("game")
            .chain(flags.iter().copied())
            .map(str::to_owned)
            .collect()
    }
    #[test]
    fn normal_launch_is_fullscreen_but_explicit_window_and_evidence_modes_are_preserved() {
        assert!(fullscreen_for(&args(&[])));
        assert!(fullscreen_for(&args(&["--mute", "--seed", "7"])));
        assert!(!fullscreen_for(&args(&["--windowed"])));
        assert!(!fullscreen_for(&args(&["--size", "800x450"])));
        assert!(fullscreen_for(&args(&[
            "--size",
            "800x450",
            "--fullscreen"
        ])));
        assert!(!fullscreen_for(&args(&["--windowed", "--fullscreen"])));
        for flag in [
            "--capture",
            "--capture-settings",
            "--script",
            "--scenario",
            "--playback",
            "--verify",
            "--probe",
            "--exit-after",
        ] {
            assert!(unattended(&args(&[flag])));
            assert!(!fullscreen_for(&args(&[flag])));
        }
        assert!(!unattended(&args(&["--mute", "--size", "800x450"])));
    }
    #[test]
    fn skip_and_timeout_both_drain_held_input_before_play() {
        let mut skipped = Intro::default();
        assert_eq!(skipped.advance(0.1, true, true), Stage::Display);
        assert_eq!(skipped.advance(0.1, true, true), Stage::Release);
        assert_eq!(skipped.advance(0.1, false, true), Stage::Release);
        assert_eq!(skipped.advance(0.1, false, false), Stage::Finished);
        let mut timed = Intro::default();
        for _ in 0..18 {
            assert_eq!(timed.advance(0.1, false, true), Stage::Display);
        }
        assert_eq!(timed.advance(0.1, false, true), Stage::Release);
        assert_eq!(timed.advance(0.1, false, false), Stage::Finished);
    }
    #[test]
    fn invalid_or_stalled_frame_clocks_cannot_skip_the_card() {
        let mut intro = Intro::default();
        for seconds in [f32::NAN, f32::INFINITY, -1., 5.] {
            assert_eq!(intro.advance(seconds, false, false), Stage::Display);
        }
        assert_eq!(intro.elapsed, 0.1);
    }
}
