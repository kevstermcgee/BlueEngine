//! UI menus and HUD screens for the game template.
use macroquad::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppScreen {
    MainMenu,
    ConnectServer,
    InLobby,
    Playing,
    Paused,
    Results,
}

pub struct MenuUi {
    pub screen: AppScreen,
    pub server_address: String,
    pub join_key: String,
    pub status_message: Option<String>,
}

impl MenuUi {
    pub fn new() -> Self {
        Self {
            screen: AppScreen::MainMenu,
            server_address: "127.0.0.1:4000".into(),
            join_key: "blue-engine-key".into(),
            status_message: None,
        }
    }

    /// Draw current screen overlay. Returns true if user clicked "Quit".
    pub fn draw(&mut self) -> bool {
        match self.screen {
            AppScreen::MainMenu => self.draw_main_menu(),
            AppScreen::ConnectServer => self.draw_connect_screen(),
            AppScreen::InLobby => self.draw_lobby_screen(),
            AppScreen::Playing => {
                self.draw_hud();
                false
            }
            AppScreen::Paused => self.draw_pause_screen(),
            AppScreen::Results => self.draw_results_screen(),
        }
    }

    fn draw_main_menu(&mut self) -> bool {
        let sw = screen_width();
        let sh = screen_height();

        draw_rectangle(0.0, 0.0, sw, sh, Color::from_rgba(20, 24, 34, 255));
        draw_text("BLUE ENGINE", sw * 0.5 - 140.0, sh * 0.25, 48.0, WHITE);
        draw_text("Multiplayer Game Shell", sw * 0.5 - 110.0, sh * 0.32, 22.0, LIGHTGRAY);

        if self.draw_button(sw * 0.5 - 100.0, sh * 0.45, 200.0, 44.0, "Play Online") {
            self.screen = AppScreen::ConnectServer;
        }
        if self.draw_button(sw * 0.5 - 100.0, sh * 0.55, 200.0, 44.0, "Explore Offline") {
            self.screen = AppScreen::Playing;
        }
        if self.draw_button(sw * 0.5 - 100.0, sh * 0.65, 200.0, 44.0, "Quit") {
            return true;
        }

        false
    }

    fn draw_connect_screen(&mut self) -> bool {
        let sw = screen_width();
        let sh = screen_height();

        draw_rectangle(0.0, 0.0, sw, sh, Color::from_rgba(20, 24, 34, 255));
        draw_text("CONNECT TO SERVER", sw * 0.5 - 150.0, sh * 0.25, 36.0, WHITE);

        draw_text(&format!("Server: {}", self.server_address), sw * 0.5 - 120.0, sh * 0.40, 20.0, LIGHTGRAY);
        draw_text(&format!("Key: {}", self.join_key), sw * 0.5 - 120.0, sh * 0.46, 20.0, LIGHTGRAY);

        if let Some(ref msg) = self.status_message {
            draw_text(msg, sw * 0.5 - 140.0, sh * 0.54, 18.0, RED);
        }

        if self.draw_button(sw * 0.5 - 100.0, sh * 0.62, 200.0, 44.0, "Connect") {
            self.screen = AppScreen::InLobby;
        }
        if self.draw_button(sw * 0.5 - 100.0, sh * 0.72, 200.0, 44.0, "Back") {
            self.screen = AppScreen::MainMenu;
        }

        false
    }

    fn draw_lobby_screen(&mut self) -> bool {
        let sw = screen_width();
        let sh = screen_height();

        draw_rectangle(0.0, 0.0, sw, sh, Color::from_rgba(20, 24, 34, 255));
        draw_text("GAME LOBBY", sw * 0.5 - 100.0, sh * 0.20, 36.0, WHITE);

        draw_text("Waiting for players to ready up...", sw * 0.5 - 130.0, sh * 0.35, 20.0, LIGHTGRAY);

        if self.draw_button(sw * 0.5 - 100.0, sh * 0.50, 200.0, 44.0, "Ready Toggle") {
            self.screen = AppScreen::Playing;
        }
        if self.draw_button(sw * 0.5 - 100.0, sh * 0.62, 200.0, 44.0, "Leave Lobby") {
            self.screen = AppScreen::MainMenu;
        }

        false
    }

    fn draw_hud(&self) {
        let sw = screen_width();
        let sh = screen_height();

        // Crosshair
        let cx = sw * 0.5;
        let cy = sh * 0.5;
        draw_line(cx - 8.0, cy, cx + 8.0, cy, 2.0, WHITE);
        draw_line(cx, cy - 8.0, cx, cy + 8.0, 2.0, WHITE);

        // Top info
        draw_text("ESC: Pause", 20.0, 30.0, 20.0, WHITE);
    }

    fn draw_pause_screen(&mut self) -> bool {
        let sw = screen_width();
        let sh = screen_height();

        draw_rectangle(0.0, 0.0, sw, sh, Color::from_rgba(10, 12, 18, 200));
        draw_text("PAUSED", sw * 0.5 - 60.0, sh * 0.30, 40.0, WHITE);

        if self.draw_button(sw * 0.5 - 100.0, sh * 0.45, 200.0, 44.0, "Resume") {
            self.screen = AppScreen::Playing;
        }
        if self.draw_button(sw * 0.5 - 100.0, sh * 0.58, 200.0, 44.0, "Disconnect") {
            self.screen = AppScreen::MainMenu;
        }

        false
    }

    fn draw_results_screen(&mut self) -> bool {
        let sw = screen_width();
        let sh = screen_height();

        draw_rectangle(0.0, 0.0, sw, sh, Color::from_rgba(20, 24, 34, 255));
        draw_text("MATCH COMPLETED", sw * 0.5 - 150.0, sh * 0.30, 40.0, GOLD);

        if self.draw_button(sw * 0.5 - 100.0, sh * 0.50, 200.0, 44.0, "Rematch (Lobby)") {
            self.screen = AppScreen::InLobby;
        }
        if self.draw_button(sw * 0.5 - 100.0, sh * 0.62, 200.0, 44.0, "Main Menu") {
            self.screen = AppScreen::MainMenu;
        }

        false
    }

    fn draw_button(&self, x: f32, y: f32, w: f32, h: f32, label: &str) -> bool {
        let (mx, my) = mouse_position();
        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;
        let bg_color = if hovered {
            Color::from_rgba(60, 90, 160, 255)
        } else {
            Color::from_rgba(40, 50, 75, 255)
        };

        draw_rectangle(x, y, w, h, bg_color);
        draw_rectangle_lines(x, y, w, h, 2.0, if hovered { WHITE } else { LIGHTGRAY });

        let font_size = 20.0;
        let text_w = label.len() as f32 * 9.0;
        draw_text(label, x + (w - text_w) * 0.5, y + h * 0.65, font_size, WHITE);

        hovered && is_mouse_button_pressed(MouseButton::Left)
    }
}
