use macroquad::prelude::*;
use sway::{audio::Playlist, Sway, MOTIONS, PALETTES, STYLES};
#[derive(Default)]
pub struct Actions {
    pub pick: bool,
    pub close: bool,
    pub style: Option<u8>,
    pub palette: Option<u8>,
    pub motion: Option<u8>,
    pub track: Option<usize>,
    pub reroll: bool,
}
const WHITE: Color = Color::new(0.9, 0.93, 1., 1.);
const MUTED: Color = Color::new(0.50, 0.55, 0.66, 1.);
fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(text, x, y, size, color);
}
pub fn button(text: &str, r: Rect, active: bool) -> bool {
    let hover = r.contains(vec2(mouse_position().0, mouse_position().1));
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        if active {
            Color::new(0.20, 0.19, 0.37, 0.96)
        } else if hover {
            Color::new(0.14, 0.16, 0.23, 0.97)
        } else {
            Color::new(0.09, 0.10, 0.16, 0.97)
        },
    );
    draw_rectangle_lines(
        r.x,
        r.y,
        r.w,
        r.h,
        1.,
        if active {
            Color::new(0.60, 0.53, 0.90, 1.)
        } else {
            Color::new(0.22, 0.24, 0.34, 0.8)
        },
    );
    let m = measure_text(text, None, 18, 1.);
    label(
        text,
        r.x + (r.w - m.width) / 2.,
        r.y + (r.h + 14.) / 2.,
        18.,
        WHITE,
    );
    hover && is_mouse_button_pressed(MouseButton::Left)
}
pub fn empty(error: &str, picking: bool, loading: bool) -> Actions {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0., 0., w, h, Color::new(0.015, 0.025, 0.055, 0.95));
    centered("Sway", w / 2., h / 2. - 100., 58., WHITE);
    centered("Your sound, in color.", w / 2., h / 2. - 55., 24., MUTED);
    let action = button(
        if picking {
            "Choosing..."
        } else if loading {
            "Opening..."
        } else {
            "Choose music"
        },
        Rect::new(w / 2. - 100., h / 2. - 15., 200., 52.),
        true,
    );
    centered(
        "or drop one or more media files anywhere",
        w / 2.,
        h / 2. + 72.,
        19.,
        MUTED,
    );
    centered(
        "MP3  WAV  FLAC  OGG  M4A  AAC  AIFF  MP4 audio",
        w / 2.,
        h / 2. + 110.,
        15.,
        MUTED,
    );
    if !error.is_empty() {
        centered(
            &short(error, 90),
            w / 2.,
            h / 2. + 155.,
            17.,
            Color::new(0.95, 0.65, 0.58, 1.),
        );
    }
    Actions {
        pick: action && !picking && !loading,
        ..Default::default()
    }
}
fn centered(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let m = measure_text(text, None, size as u16, 1.);
    label(text, x - m.width / 2., y, size, color);
}
pub fn options(model: &Sway, list: &Playlist, error: &str, picking: bool) -> Actions {
    let mut a = Actions::default();
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0., 0., w, h, Color::new(0.005, 0.01, 0.025, 0.82));
    let scale = (w / 1000.).min(h / 660.).min(1.5);
    let pw = 900. * scale;
    let ph = 610. * scale;
    let ox = (w - pw) / 2.;
    let oy = (h - ph) / 2.;
    // Logical panel keeps all choices reachable on small windows and high DPI displays.
    let mut cam = Camera2D::from_display_rect(Rect::new(0., 0., 900., 610.));
    // This pinned backend flips screen-camera Y; keep text and hit boxes aligned.
    cam.zoom.y = cam.zoom.y.abs();
    let dpi = screen_dpi_scale();
    cam.viewport = Some((
        (ox * dpi) as i32,
        ((h - oy - ph) * dpi) as i32,
        (pw * dpi) as i32,
        (ph * dpi) as i32,
    ));
    set_camera(&cam);
    draw_rectangle(0., 0., 900., 610., Color::new(0.035, 0.042, 0.073, 0.98));
    label("Sway", 30., 52., 38., WHITE);
    label("Make this listen yours.", 30., 83., 19., MUTED);
    let mouse = vec2(
        (mouse_position().0 - ox) / scale,
        (mouse_position().1 - oy) / scale,
    );
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    let logical_button = |text: &str, r: Rect, active: bool| {
        let hover = r.contains(mouse);
        draw_rectangle(
            r.x,
            r.y,
            r.w,
            r.h,
            if active {
                Color::new(0.21, 0.18, 0.37, 1.)
            } else if hover {
                Color::new(0.15, 0.17, 0.26, 1.)
            } else {
                Color::new(0.08, 0.09, 0.15, 1.)
            },
        );
        draw_rectangle_lines(
            r.x,
            r.y,
            r.w,
            r.h,
            1.,
            if active {
                Color::new(0.6, 0.52, 0.88, 1.)
            } else {
                Color::new(0.19, 0.22, 0.31, 1.)
            },
        );
        let m = measure_text(text, None, 17, 1.);
        label(
            text,
            r.x + (r.w - m.width) / 2.,
            r.y + (r.h + 13.) / 2.,
            17.,
            WHITE,
        );
        hover && clicked
    };
    a.close = logical_button("Back", Rect::new(766., 27., 102., 40.), false);
    a.pick = !picking && logical_button("Add music", Rect::new(30., 110., 160., 42.), false);
    a.reroll = logical_button("Fresh look", Rect::new(207., 110., 160., 42.), false);
    label("VISUALS", 30., 197., 14., MUTED);
    for (i, s) in STYLES.iter().enumerate() {
        if logical_button(
            s,
            Rect::new(30. + i as f32 * 121., 214., 114., 39.),
            model.state.style == i as u8,
        ) {
            a.style = Some(i as u8);
        }
    }
    label("COLOR", 30., 288., 14., MUTED);
    for (i, s) in PALETTES.iter().enumerate() {
        if logical_button(
            s,
            Rect::new(30. + i as f32 * 121., 305., 114., 39.),
            model.state.palette == i as u8,
        ) {
            a.palette = Some(i as u8);
        }
    }
    label("MOTION", 30., 379., 14., MUTED);
    for (i, s) in MOTIONS.iter().enumerate() {
        if logical_button(
            s,
            Rect::new(30. + i as f32 * 121., 396., 114., 39.),
            model.state.motion == i as u8,
        ) {
            a.motion = Some(i as u8);
        }
    }
    label(
        &format!("PLAYLIST · {} tracks", list.files.len()),
        30.,
        474.,
        14.,
        MUTED,
    );
    let start = list.current.saturating_sub(1);
    for (row, index) in (start..list.files.len()).take(3).enumerate() {
        let title = list.files[index]
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if logical_button(
            &short(&title, 75),
            Rect::new(30., 487. + row as f32 * 27., 838., 25.),
            index == list.current,
        ) {
            a.track = Some(index);
        }
    }
    if error.is_empty() {
        label("Space pause/resume   ·   Arrows track   ·   O add files   ·   R fresh look   ·   F fullscreen",30.,590.,15.,MUTED);
    } else {
        label(
            &short(error, 105),
            30.,
            590.,
            15.,
            Color::new(0.95, 0.65, 0.58, 1.),
        );
    }
    set_default_camera();
    a
}
pub fn title(text: &str, elapsed: f32) {
    let alpha = Sway::title_alpha(elapsed);
    if alpha <= 0. {
        return;
    }
    let text = short(text, 80);
    draw_text(
        &text,
        30.,
        screen_height() - 30.,
        20.,
        Color::new(0., 0., 0., alpha * 0.4),
    );
    draw_text(
        &text,
        29.,
        screen_height() - 31.,
        20.,
        Color::new(0.88, 0.90, 0.95, alpha * 0.8),
    );
}
pub fn short(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}
