//! Shared immediate-mode game UI. One navigation scope per window/render thread.
//! Call begin_navigation before enabled buttons and end_navigation afterwards.
#[derive(Default, Clone, Copy)]
pub struct NavigationInput {
    pub next: bool,
    pub previous: bool,
    pub accept: bool,
}
use super::game_text;
use macroquad::prelude as mq;

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
    let (pad_focus, pad_click) = navigation_button(enabled);
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
    if pad_focus {
        mq::draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 3., ACCENT);
    }
    enabled && ((hover && mq::is_mouse_button_pressed(mq::MouseButton::Left)) || pad_click)
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

#[derive(Default)]
struct Navigation {
    surface: u8,
    focus: usize,
    count: usize,
    next: usize,
    active: bool,
    accept: bool,
}
thread_local! { static NAV: std::cell::RefCell<Navigation> = std::cell::RefCell::new(Navigation::default()); }
/// Enumerate enabled controls once per frame. Overlays use distinct focus scopes.
pub fn begin_navigation(surface: u8, input: NavigationInput) {
    NAV.with(|cell| {
        let mut n = cell.borrow_mut();
        if n.surface != surface {
            *n = Navigation {
                surface,
                ..Default::default()
            };
        }
        let next = input.next;
        let previous = input.previous;
        if next || previous {
            n.active = true;
            let count = n.count.max(1);
            n.focus = if next {
                (n.focus + 1) % count
            } else {
                (n.focus + count - 1) % count
            };
        }
        n.accept = surface != 0 && input.accept;
        if n.accept {
            n.active = true;
        }
        n.focus = n.focus.min(n.count.saturating_sub(1));
        n.next = 0;
    });
}
fn navigation_button(enabled: bool) -> (bool, bool) {
    NAV.with(|cell| {
        let mut n = cell.borrow_mut();
        if !enabled || n.surface == 0 {
            return (false, false);
        }
        let focused = n.active && n.next == n.focus;
        n.next += 1;
        let click = focused && n.accept;
        if click {
            n.accept = false;
        }
        (focused, click)
    })
}
pub fn end_navigation() {
    NAV.with(|n| {
        let mut n = n.borrow_mut();
        n.count = n.next;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_focused_enabled_control_consumes_confirmation() {
        NAV.with(|n| {
            *n.borrow_mut() = Navigation {
                surface: 2,
                focus: 1,
                active: true,
                accept: true,
                ..Default::default()
            }
        });
        assert_eq!(navigation_button(false), (false, false));
        assert_eq!(navigation_button(true), (false, false));
        assert_eq!(navigation_button(true), (true, true));
        assert_eq!(navigation_button(true), (false, false));
        end_navigation();
        NAV.with(|n| {
            let n = n.borrow();
            assert_eq!(n.count, 3);
            assert!(!n.accept);
        });
    }
    #[test]
    fn paused_scope_does_not_activate_underlying_controls() {
        NAV.with(|n| {
            *n.borrow_mut() = Navigation {
                active: true,
                accept: true,
                ..Default::default()
            }
        });
        assert_eq!(navigation_button(true), (false, false));
    }
}
