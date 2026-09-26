use macroquad::prelude as mq;
use vesper3d::viewer::game_text;

pub const PAPER: mq::Color = mq::Color::new(0.94, 0.95, 0.92, 1.);
pub const INK: mq::Color = mq::Color::new(0.08, 0.16, 0.22, 1.);
pub const MUTED: mq::Color = mq::Color::new(0.35, 0.43, 0.46, 1.);
pub const BLUE: mq::Color = mq::Color::new(0.08, 0.33, 0.56, 1.);
pub const ACCENT: mq::Color = mq::Color::new(0.55, 0.91, 0.80, 1.);
pub fn text(s: &str, x: f32, y: f32, size: f32, color: mq::Color) {
    game_text::draw_text(s, x, y, size, color);
}
pub fn panel(x: f32, y: f32, w: f32, h: f32, color: mq::Color) {
    mq::draw_rectangle(x, y, w, h, color);
}
pub fn button(label: &str, rect: mq::Rect, selected: bool, enabled: bool) -> bool {
    let hover = rect.contains(mq::Vec2::from(mq::mouse_position()));
    let active = selected || (hover && enabled);
    panel(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        if active {
            BLUE
        } else {
            mq::Color::new(0.85, 0.89, 0.88, 1.)
        },
    );
    let size = (rect.h * 0.45).min(19.);
    let width = game_text::measure_text(label, None, size as u16, 1.).width;
    let size = size * ((rect.w - 24.) / width.max(1.)).min(1.);
    text(
        label,
        rect.x + 12.,
        rect.y + rect.h * 0.65,
        size,
        if active {
            mq::WHITE
        } else if enabled {
            INK
        } else {
            MUTED
        },
    );
    enabled && hover && mq::is_mouse_button_pressed(mq::MouseButton::Left)
}
pub fn fit(s: &str, width: f32, size: f32) -> String {
    let mut out = s.to_owned();
    if game_text::measure_text(&out, None, size as u16, 1.).width <= width {
        return out;
    }
    while !out.is_empty()
        && game_text::measure_text(&(out.clone() + "..."), None, size as u16, 1.).width > width
    {
        out.pop();
    }
    out + "..."
}
pub fn paragraph(s: &str, x: f32, mut y: f32, width: f32, size: f32) {
    let mut line = String::new();
    for word in s.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if game_text::measure_text(&candidate, None, size as u16, 1.).width > width
            && !line.is_empty()
        {
            text(&line, x, y, size, MUTED);
            y += size * 1.35;
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }
    text(&line, x, y, size, MUTED);
}
