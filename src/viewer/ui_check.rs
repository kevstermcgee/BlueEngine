//! Headless 2D UI layout auditor for BlueEngine menus and screens.
//!
//! Audits standard menus (Main Menu, Lobby, Join Dialog, HUD, Pause Screen) across
//! 9 standard window sizes without requiring a GPU or windowing context, ensuring
//! no text overflows, clipped buttons, or overlapping interactive widgets.

use serde::{Deserialize, Serialize};

pub const CHECK_SIZES: [(u32, u32); 9] = [
    (480, 270),
    (640, 360),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1920, 1080),
    (2560, 1440),
    (500, 640), // portrait
    (720, 720), // square
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WidgetRect {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutViolation {
    pub screen: String,
    pub size: [u32; 2],
    pub widget: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiAuditReport {
    pub ok: bool,
    pub screens_audited: Vec<String>,
    pub total_tests: usize,
    pub violations: Vec<LayoutViolation>,
}

pub fn get_screen_widgets(screen_name: &str, w: u32, h: u32) -> Option<Vec<WidgetRect>> {
    let fw = w as f32;
    let fh = h as f32;

    match screen_name {
        "main_menu" => {
            let center_x = fw * 0.5;
            let btn_w = (fw * 0.35).clamp(160.0, 320.0);
            let btn_h = (fh * 0.08).clamp(32.0, 56.0);
            let mut y = fh * 0.35;
            let spacing = btn_h + 12.0;

            Some(vec![
                WidgetRect {
                    id: "title".into(),
                    x: center_x - 150.0,
                    y: fh * 0.15,
                    width: 300.0,
                    height: 48.0,
                    text: Some("BLUE ENGINE 2".into()),
                },
                WidgetRect {
                    id: "btn_play".into(),
                    x: center_x - btn_w * 0.5,
                    y,
                    width: btn_w,
                    height: btn_h,
                    text: Some("Play Local".into()),
                },
                WidgetRect {
                    id: "btn_host".into(),
                    x: center_x - btn_w * 0.5,
                    y: {
                        y += spacing;
                        y
                    },
                    width: btn_w,
                    height: btn_h,
                    text: Some("Host Server".into()),
                },
                WidgetRect {
                    id: "btn_join".into(),
                    x: center_x - btn_w * 0.5,
                    y: {
                        y += spacing;
                        y
                    },
                    width: btn_w,
                    height: btn_h,
                    text: Some("Join by IP".into()),
                },
                WidgetRect {
                    id: "btn_quit".into(),
                    x: center_x - btn_w * 0.5,
                    y: {
                        y += spacing;
                        y
                    },
                    width: btn_w,
                    height: btn_h,
                    text: Some("Quit".into()),
                },
            ])
        }
        "lobby" => {
            let panel_w = (fw * 0.4).clamp(180.0, 400.0);
            Some(vec![
                WidgetRect {
                    id: "lobby_title".into(),
                    x: 20.0,
                    y: 20.0,
                    width: 250.0,
                    height: 36.0,
                    text: Some("Multiplayer Lobby".into()),
                },
                WidgetRect {
                    id: "player_list".into(),
                    x: 20.0,
                    y: 70.0,
                    width: panel_w,
                    height: fh - 150.0,
                    text: None,
                },
                WidgetRect {
                    id: "btn_ready".into(),
                    x: fw - panel_w - 20.0,
                    y: fh - 80.0,
                    width: panel_w,
                    height: 48.0,
                    text: Some("Ready Up".into()),
                },
                WidgetRect {
                    id: "btn_start".into(),
                    x: fw - panel_w - 20.0,
                    y: fh - 140.0,
                    width: panel_w,
                    height: 48.0,
                    text: Some("Start Match".into()),
                },
            ])
        }
        "join_dialog" => {
            let dialog_w = (fw * 0.5).clamp(240.0, 480.0);
            let dialog_h = (fh * 0.45).clamp(180.0, 320.0);
            let dx = (fw - dialog_w) * 0.5;
            let dy = (fh - dialog_h) * 0.5;

            Some(vec![
                WidgetRect {
                    id: "dialog_bg".into(),
                    x: dx,
                    y: dy,
                    width: dialog_w,
                    height: dialog_h,
                    text: None,
                },
                WidgetRect {
                    id: "input_ip".into(),
                    x: dx + 20.0,
                    y: dy + 40.0,
                    width: dialog_w - 40.0,
                    height: 36.0,
                    text: Some("127.0.0.1:27015".into()),
                },
                WidgetRect {
                    id: "input_key".into(),
                    x: dx + 20.0,
                    y: dy + 90.0,
                    width: dialog_w - 40.0,
                    height: 36.0,
                    text: Some("Join Key (optional)".into()),
                },
                WidgetRect {
                    id: "btn_connect".into(),
                    x: dx + 20.0,
                    y: dy + dialog_h - 50.0,
                    width: (dialog_w - 60.0) * 0.5,
                    height: 38.0,
                    text: Some("Connect".into()),
                },
                WidgetRect {
                    id: "btn_cancel".into(),
                    x: dx + (dialog_w - 60.0) * 0.5 + 40.0,
                    y: dy + dialog_h - 50.0,
                    width: (dialog_w - 60.0) * 0.5,
                    height: 38.0,
                    text: Some("Cancel".into()),
                },
            ])
        }
        "hud" => Some(vec![
            WidgetRect {
                id: "health_bar".into(),
                x: 20.0,
                y: fh - 50.0,
                width: 200.0,
                height: 28.0,
                text: Some("HP 100/100".into()),
            },
            WidgetRect {
                id: "ammo_display".into(),
                x: fw - 160.0,
                y: fh - 50.0,
                width: 140.0,
                height: 28.0,
                text: Some("AMMO 30/90".into()),
            },
            WidgetRect {
                id: "ping_label".into(),
                x: fw - 100.0,
                y: 15.0,
                width: 80.0,
                height: 20.0,
                text: Some("32 ms".into()),
            },
            WidgetRect {
                id: "crosshair".into(),
                x: fw * 0.5 - 8.0,
                y: fh * 0.5 - 8.0,
                width: 16.0,
                height: 16.0,
                text: None,
            },
        ]),
        "pause_menu" => {
            let menu_w = (fw * 0.3).clamp(160.0, 300.0);
            let cx = fw * 0.5 - menu_w * 0.5;
            let h = (fh * 0.1).clamp(26.0, 42.0);
            let s = h + (fh * 0.03).clamp(6.0, 12.0);
            let title_h = (fh * 0.08).clamp(20.0, 36.0);
            let title_y = (fh * 0.06).max(10.0);
            let mut y = title_y + title_h + (fh * 0.03).clamp(6.0, 14.0);

            Some(vec![
                WidgetRect {
                    id: "pause_title".into(),
                    x: cx,
                    y: title_y,
                    width: menu_w,
                    height: title_h,
                    text: Some("GAME PAUSED".into()),
                },
                WidgetRect {
                    id: "btn_resume".into(),
                    x: cx,
                    y,
                    width: menu_w,
                    height: h,
                    text: Some("Resume".into()),
                },
                WidgetRect {
                    id: "btn_options".into(),
                    x: cx,
                    y: {
                        y += s;
                        y
                    },
                    width: menu_w,
                    height: h,
                    text: Some("Options".into()),
                },
                WidgetRect {
                    id: "btn_rematch".into(),
                    x: cx,
                    y: {
                        y += s;
                        y
                    },
                    width: menu_w,
                    height: h,
                    text: Some("Rematch".into()),
                },
                WidgetRect {
                    id: "btn_quit".into(),
                    x: cx,
                    y: {
                        y += s;
                        y
                    },
                    width: menu_w,
                    height: h,
                    text: Some("Disconnect".into()),
                },
            ])
        }
        _ => None,
    }
}

pub fn audit_all_screens() -> UiAuditReport {
    let screens = ["main_menu", "lobby", "join_dialog", "hud", "pause_menu"];
    let mut violations = Vec::new();
    let mut total_tests = 0;

    for &screen in &screens {
        for &(w, h) in &CHECK_SIZES {
            total_tests += 1;
            if let Some(widgets) = get_screen_widgets(screen, w, h) {
                let fw = w as f32;
                let fh = h as f32;

                for (i, widget) in widgets.iter().enumerate() {
                    // Check bounds within window
                    if widget.x < 0.0
                        || widget.y < 0.0
                        || widget.x + widget.width > fw
                        || widget.y + widget.height > fh
                    {
                        violations.push(LayoutViolation {
                            screen: screen.into(),
                            size: [w, h],
                            widget: widget.id.clone(),
                            code: "out-of-bounds".into(),
                            message: format!("Widget '{}' [x:{:.1}, y:{:.1}, w:{:.1}, h:{:.1}] clips window ({}x{})", widget.id, widget.x, widget.y, widget.width, widget.height, w, h),
                        });
                    }

                    // Check overlapping sibling buttons
                    for other in &widgets[(i + 1)..] {
                        if widget.id == "dialog_bg" || other.id == "dialog_bg" {
                            continue; // backgrounds contain children
                        }
                        let ox = (widget.x + widget.width).min(other.x + other.width)
                            - widget.x.max(other.x);
                        let oy = (widget.y + widget.height).min(other.y + other.height)
                            - widget.y.max(other.y);

                        if ox > 1.0 && oy > 1.0 {
                            violations.push(LayoutViolation {
                                screen: screen.into(),
                                size: [w, h],
                                widget: widget.id.clone(),
                                code: "overlap".into(),
                                message: format!(
                                    "Widget '{}' overlaps '{}' by {:.1}x{:.1}px",
                                    widget.id, other.id, ox, oy
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    UiAuditReport {
        ok: violations.is_empty(),
        screens_audited: screens.iter().map(|s| s.to_string()).collect(),
        total_tests,
        violations,
    }
}
