//! 2D drawing helpers for heads-up displays: scaled, outlined, centred text, panels, bars, a vignette,
//! world-anchored popups and banners.
//!
//! Text uses the engine's immutable `game_text` atlas, so it stays sharp and never reallocates a GPU
//! texture while a frame is drawn. Sizes are in pixels at a 720-pixel-high window; multiply by
//! [`ui_scale`] so the layout follows the window (and fullscreen).
use super::batch::Rgb;
use super::fx::{Banner, Popup};
use super::view::View;
use crate::viewer::game_text;
use macroquad::prelude::*;

/// A colour from linear RGB plus alpha.
pub fn col(c: Rgb, a: f32) -> Color {
    Color::new(c[0], c[1], c[2], a)
}

/// UI scale from the window height: 1.0 at 720 px, clamped to 0.55-2.4.
pub fn ui_scale() -> f32 {
    (screen_height() / 720.).clamp(0.55, 2.4)
}

/// Width of `text` at `size` pixels.
pub fn width_of(text: &str, size: f32) -> f32 {
    game_text::measure_text(text, None, size as u16, 1.).width
}

/// Text with a dark outline (readable on any background) and its baseline at `y`.
pub fn text_outlined(text: &str, x: f32, y: f32, size: f32, color: Color) {
    let d = (size * 0.05).max(1.5);
    let outline = Color::new(0.02, 0.0, 0.06, color.a * 0.9);
    for (dx, dy) in [
        (-d, 0.),
        (d, 0.),
        (0., -d),
        (0., d),
        (-d, -d),
        (d, d),
        (-d, d),
        (d, -d),
    ] {
        game_text::draw_text(text, x + dx, y + dy, size, outline);
    }
    game_text::draw_text(text, x, y, size, color);
}

/// Outlined text centred on `cx`.
pub fn text_centered(text: &str, cx: f32, y: f32, size: f32, color: Color) {
    text_outlined(text, cx - width_of(text, size) * 0.5, y, size, color);
}

/// Outlined text whose right edge is at `right`.
pub fn text_right(text: &str, right: f32, y: f32, size: f32, color: Color) {
    text_outlined(text, right - width_of(text, size), y, size, color);
}

/// `12345` as `12,345`.
pub fn commas(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Split `text` into lines no wider than `width` pixels at `size` (a single longer word stays whole).
pub fn wrap(text: &str, width: f32, size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if width_of(&candidate, size) > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A slanted arcade-style panel (`skew` pixels of lean; 0 for a plain rectangle).
pub fn panel(x: f32, y: f32, w: f32, h: f32, skew: f32, color: Color) {
    let (a, b, c, d) = (
        vec2(x + skew, y),
        vec2(x + w + skew, y),
        vec2(x + w, y + h),
        vec2(x, y + h),
    );
    draw_triangle(a, b, c, color);
    draw_triangle(a, c, d, color);
}

/// A horizontal meter: a dark track with `fill` (0-1) of it in `color`.
pub fn bar(x: f32, y: f32, w: f32, h: f32, fill: f32, color: Color) {
    draw_rectangle(x, y, w, h, Color::new(0., 0., 0., 0.55));
    draw_rectangle(x, y, w * fill.clamp(0., 1.), h, color);
}

/// A plus-shaped crosshair at the screen centre. `spread` opens it (recoil, movement).
pub fn crosshair(scale: f32, spread: f32, color: Color) {
    let (cx, cy) = (screen_width() * 0.5, screen_height() * 0.5);
    let (len, gap, thick) = (9. * scale, (4. + spread) * scale, 2.2 * scale);
    let shadow = Color::new(0., 0., 0., color.a * 0.6);
    for (c, off) in [(shadow, 1.2 * scale), (color, 0.)] {
        draw_rectangle(cx - gap - len + off, cy - thick * 0.5 + off, len, thick, c);
        draw_rectangle(cx + gap + off, cy - thick * 0.5 + off, len, thick, c);
        draw_rectangle(cx - thick * 0.5 + off, cy - gap - len + off, thick, len, c);
        draw_rectangle(cx - thick * 0.5 + off, cy + gap + off, thick, len, c);
    }
}

/// A soft radial vignette: transparent in the middle, dark at the edges. Draw it stretched over the
/// screen with [`overlay`], tinted for damage (red) or shields (blue).
pub fn make_vignette() -> Texture2D {
    let n = 128u16;
    let mut img = Image::gen_image_color(n, n, Color::new(1., 1., 1., 0.));
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (
                (x as f32 / (n - 1) as f32) * 2. - 1.,
                (y as f32 / (n - 1) as f32) * 2. - 1.,
            );
            let r = (fx * fx + fy * fy).sqrt() / std::f32::consts::SQRT_2;
            let a = ((r - 0.35) / 0.65).clamp(0., 1.).powf(1.7);
            img.set_pixel(x as u32, y as u32, Color::new(1., 1., 1., a));
        }
    }
    let texture = Texture2D::from_image(&img);
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Stretch a texture (a vignette) over the whole screen with a tint.
pub fn overlay(texture: &Texture2D, tint: Color) {
    draw_texture_ex(
        texture,
        0.,
        0.,
        tint,
        DrawTextureParams {
            dest_size: Some(vec2(screen_width(), screen_height())),
            ..Default::default()
        },
    );
}

/// Draw [`Fx`](super::Fx) popups anchored at their world positions, popping in and fading out.
pub fn draw_popups(popups: &[Popup], view: &View, scale: f32) {
    let (w, h) = (screen_width(), screen_height());
    for p in popups {
        let Some(at) = view.project(p.pos, w, h) else {
            continue;
        };
        let t = 1. - p.life / p.max;
        let pop = 0.7 + 0.5 * (1. - (t * 6.).min(1.)).powi(2) + 0.3 * (t * 6.).min(1.);
        let fade = ((p.life / p.max) * 3.).min(1.);
        let size = p.size * scale * pop.min(1.3);
        for (i, line) in p.text.split('\n').enumerate() {
            text_centered(
                line,
                at.x,
                at.y + i as f32 * size * 0.92,
                size,
                col(p.color, fade),
            );
        }
    }
}

/// Draw [`Fx`](super::Fx) banners stacked below the upper third of the screen.
pub fn draw_banners(banners: &[Banner], scale: f32) {
    let (w, h) = (screen_width(), screen_height());
    for (i, b) in banners.iter().enumerate() {
        let t = 1. - b.life / b.max;
        let pop = 1. + (1. - (t * 7.).min(1.)).powi(2) * 0.6;
        let fade = ((b.life / b.max) * 4.).min(1.);
        let y = h * 0.27 + i as f32 * 64. * scale;
        text_centered(&b.text, w * 0.5, y, 74. * scale * pop, col(b.color, fade));
        if !b.sub.is_empty() {
            text_centered(
                &b.sub,
                w * 0.5,
                y + 40. * scale,
                28. * scale,
                col([1.; 3], fade),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas_group_thousands() {
        for (n, text) in [
            (0, "0"),
            (7, "7"),
            (999, "999"),
            (1000, "1,000"),
            (12_345, "12,345"),
            (1_234_567, "1,234,567"),
            (u64::MAX, "18,446,744,073,709,551,615"),
        ] {
            assert_eq!(commas(n), text);
        }
    }

    #[test]
    fn wrapping_respects_the_width_and_keeps_every_word() {
        let text = "the quick brown fox jumps over the lazy dog";
        let lines = wrap(text, 200., 24.);
        assert!(lines.len() > 1);
        assert!(
            lines
                .iter()
                .all(|l| width_of(l, 24.) <= 200. || !l.contains(' ')),
            "{lines:?}"
        );
        assert_eq!(lines.join(" "), text);
        assert!(wrap("", 100., 20.).is_empty());
        assert_eq!(
            wrap("supercalifragilistic", 10., 30.),
            ["supercalifragilistic"],
            "a long word stays whole"
        );
    }

    #[test]
    fn text_width_grows_with_size_and_length() {
        assert!(width_of("WIDER", 40.) > width_of("W", 40.));
        assert!(width_of("text", 40.) > width_of("text", 20.) * 1.9);
        assert_eq!(width_of("", 30.), 0.);
    }

    #[test]
    fn colours_carry_alpha() {
        let c = col([0.1, 0.2, 0.3], 0.4);
        assert_eq!((c.r, c.g, c.b, c.a), (0.1, 0.2, 0.3, 0.4));
    }
}
