//! Shared, opt-in presentation defaults for standalone games.
//! F/F11 changes fullscreen in place. Escape opens the menu and gates all gameplay
//! input. Online authority continues while the menu is open. No OS unsafe calls.
use super::game_text::draw_text;
use super::{controller::Collider, mesh};
use crate::{geometry::World, math::V};
use macroquad::{
    input::utils::{register_input_subscriber, repeat_all_miniquad_input},
    prelude::*,
};

/// Window settings shared by the stock client and generated games. The window carries miniquad's
/// default icon: a game that ships should give it its own with [`window_config_with_icon`].
static EXIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Ask the game loop to finish: from a Quit button, a controller shortcut, a network "kicked" event.
/// It only sets a flag; the loop ends at the point it checks [`exit_requested`], so `main` returns
/// normally, destructors run and a networked game can say goodbye first. Prefer it to
/// `std::process::exit`, which skips all of that.
///
/// ```ignore
/// loop {
///     if game_client::exit_requested() { break; }   // top of the frame
///     /* ... input, update, draw; a Quit button calls game_client::request_exit() ... */
///     next_frame().await;
/// }
/// net.send_disconnect();   // runs: the loop ended, main did not abort
/// ```
pub fn request_exit() {
    EXIT_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
}
/// True once [`request_exit`] was called. Check it once per frame and `break` out of the loop.
pub fn exit_requested() -> bool {
    EXIT_REQUESTED.load(std::sync::atomic::Ordering::SeqCst)
}
pub fn window_config(title: &str) -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: title.into(),
            window_width: 1280,
            window_height: 720,
            high_dpi: true,
            sample_count: 4,
            ..Default::default()
        },
        draw_call_vertex_capacity: 30000,
        draw_call_index_capacity: 30000,
        ..Default::default()
    }
}

/// [`window_config`] with the game's own title-bar and taskbar icon. The title should be the game's
/// `identity.json` title, the same text as its desktop shortcut.
pub fn window_config_with_icon(
    title: &str,
    icon: macroquad::miniquad::conf::Icon,
) -> macroquad::conf::Conf {
    let mut conf = window_config(title);
    conf.miniquad_conf.icon = Some(icon);
    conf
}

/// The window icon from the three raw RGBA blobs `be2-tools icon` writes next to the `.ico`
/// (`assets/icon_16.rgba`, `icon_32.rgba`, `icon_64.rgba`). The array types make a blob of the wrong
/// size a compile error when it comes straight from `include_bytes!`:
///
/// ```ignore
/// let icon = game_client::icon_from_rgba(
///     include_bytes!("../assets/icon_16.rgba"),
///     include_bytes!("../assets/icon_32.rgba"),
///     include_bytes!("../assets/icon_64.rgba"),
/// );
/// ```
pub fn icon_from_rgba(
    small: &[u8; 1024],
    medium: &[u8; 4096],
    big: &[u8; 16384],
) -> macroquad::miniquad::conf::Icon {
    macroquad::miniquad::conf::Icon {
        small: *small,
        medium: *medium,
        big: *big,
    }
}

/// Cache transformed, shaded primitives once, including cylinders and cones.
pub fn static_meshes(world: &World) -> Vec<Mesh> {
    mesh::bake_tagged(
        world,
        &[(
            Collider {
                min: V(-100000., -100000., -100000.),
                max: V(100000., 100000., 100000.),
            },
            3.,
        )],
    )
}

struct Focus {
    active: bool,
}
impl miniquad::EventHandler for Focus {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn window_minimized_event(&mut self) {
        self.active = false;
    }
    fn window_restored_event(&mut self) {
        self.active = true;
    }
}

/// One frame of focus-aware menu edges. Never store held actions as presses.
#[derive(Default, Clone, Copy)]
pub struct ShellActions {
    pub fullscreen: bool,
    pub pause: bool,
    pub diagnostics: bool,
    pub release_cursor: bool,
    pub next: bool,
    pub previous: bool,
    pub accept: bool,
}
impl ShellActions {
    pub fn from_keys(pressed: impl Fn(KeyCode) -> bool) -> Self {
        Self {
            fullscreen: pressed(KeyCode::F) || pressed(KeyCode::F11),
            pause: pressed(KeyCode::Escape),
            diagnostics: pressed(KeyCode::F3),
            release_cursor: pressed(KeyCode::LeftAlt),
            next: pressed(KeyCode::Down),
            previous: pressed(KeyCode::Up),
            accept: pressed(KeyCode::Enter),
        }
    }
}
/// What the settings screen shows: whether music/sound effects currently play. The shell owns no
/// volume or persistence; the game supplies this each frame and acts on the returned [`MenuOutcome`].
#[derive(Clone, Copy, Default)]
pub struct AudioMenu {
    pub music_on: bool,
    pub sfx_on: bool,
    /// Whether this game has any music at all. Not every game needs one: generated ambient music can
    /// interfere with a gameplay mechanic (precise audio cues, rhythm timing, a game's own diegetic
    /// sound design) or simply not fit. When `false`, the Settings screen shows only the sound toggle;
    /// it does not offer a music toggle or "Save music" button for a track that does not exist.
    pub has_music: bool,
}
/// What the player did with [`GameShell::local_menu_with_audio`] this frame. At most the fields that
/// actually changed are set; a game applies each one it cares about and persists what it changes.
#[derive(Clone, Copy, Default)]
pub struct MenuOutcome {
    /// The player chose Quit.
    pub quit: bool,
    /// The player clicked the music toggle; flip the setting and re-apply its volume.
    pub toggle_music: bool,
    /// The player clicked the sound toggle; flip the setting and re-apply its volume.
    pub toggle_sfx: bool,
    /// The player clicked "Save music"; write the current track to a file (see
    /// [`super::devkit::downloads_dir`]) and tell them whether it worked.
    pub download_music: bool,
    /// The player clicked the shadows selector (only offered by
    /// [`GameShell::local_menu_with_options`]); step the setting with `Settings::cycle_shadow_quality`,
    /// apply it to `kit::Shadows::set_quality` and persist it.
    pub cycle_shadows: bool,
}
pub struct GameShell {
    pub paused: bool,
    pub fullscreen: bool,
    pub diagnostics: bool,
    subscriber: usize,
    focus: Focus,
    captured: bool,
    controls: bool,
    settings_screen: bool,
    selection: usize,
    suppress: bool,
    actions: ShellActions,
}
impl Default for GameShell {
    fn default() -> Self {
        Self::new()
    }
}
impl GameShell {
    pub fn new() -> Self {
        Self {
            paused: false,
            fullscreen: false,
            diagnostics: false,
            subscriber: register_input_subscriber(),
            focus: Focus { active: true },
            captured: false,
            controls: false,
            settings_screen: false,
            selection: 0,
            suppress: false,
            actions: ShellActions::default(),
        }
    }
    /// `capture_cursor` is whether the game wants the mouse captured while it is unpaused and focused:
    /// `true` in play, `false` on a title or menu screen so the cursor is released and clickable. (An
    /// online session passes "connected" here, hence the old name.)
    pub fn begin_frame(&mut self, capture_cursor: bool) {
        self.begin_frame_with_input(capture_cursor, is_key_pressed);
    }
    /// An executable can supply focus-aware native key edges without unsafe library code.
    pub fn begin_frame_with_input(&mut self, capture_cursor: bool, pressed: fn(KeyCode) -> bool) {
        self.begin_frame_with_actions(capture_cursor, true, ShellActions::from_keys(pressed));
    }
    /// Combine native focus and device actions. Poll input even while menus are open.
    /// `capture_cursor` is as for [`GameShell::begin_frame`].
    pub fn begin_frame_with_actions(
        &mut self,
        capture_cursor: bool,
        focused: bool,
        actions: ShellActions,
    ) {
        self.actions = if focused {
            actions
        } else {
            ShellActions::default()
        };
        repeat_all_miniquad_input(&mut self.focus, self.subscriber);
        self.suppress = false;
        if !self.focus.active || !focused {
            self.paused = true;
        }
        if self.focus.active && self.actions.fullscreen {
            self.fullscreen = !self.fullscreen;
            set_fullscreen(self.fullscreen);
            self.suppress = true;
        }
        if self.focus.active && self.actions.pause {
            self.paused = !self.paused;
            self.controls = false;
            self.settings_screen = false;
            self.suppress = true;
        }
        if self.actions.diagnostics {
            self.diagnostics = !self.diagnostics;
        }
        if self.actions.release_cursor {
            self.paused = true;
        }
        let capture = capture_cursor && !self.paused && self.focus.active && focused;
        if capture != self.captured {
            set_cursor_grab(capture);
            show_mouse(!capture);
            self.captured = capture;
            self.suppress = true;
        }
    }
    /// A shell in a chosen state without a window: the real constructor registers a macroquad input subscriber.
    #[cfg(test)]
    pub(crate) fn in_state(captured: bool, paused: bool, suppress: bool) -> Self {
        Self {
            paused,
            fullscreen: false,
            diagnostics: false,
            subscriber: 0,
            focus: Focus { active: true },
            captured,
            controls: false,
            settings_screen: false,
            selection: 0,
            suppress,
            actions: ShellActions::default(),
        }
    }
    /// True only while the mouse is **captured**, the menu is closed and no shell key was just handled.
    ///
    /// A game with no mouse look never asks for capture (`capture_cursor = false`), so for it this is always
    /// false: gating input on it silently drops every key and button (menus that read keys directly still work).
    /// Such games want [`GameShell::accepting_input`].
    pub fn playing(&self) -> bool {
        self.captured && !self.paused && !self.suppress
    }
    /// Whether the game should read its devices this frame regardless of mouse capture: the pause menu is closed
    /// and no shell key was just handled. The right gate for a game with no mouse look (a kart racer, a menu-driven
    /// puzzle); a first-person game uses [`GameShell::playing`], which also needs the mouse captured.
    pub fn accepting_input(&self) -> bool {
        !self.paused && !self.suppress
    }
    /// Draw a small, keyboard/mouse accessible overlay. Returns true on Quit.
    pub fn menu(&mut self, title: &str, controls: &[&str]) -> bool {
        self.menu_with_status(title, controls, "MENU  /  ONLINE MATCH CONTINUES")
    }
    /// Shared pause menu for an application that pauses its local simulation.
    pub fn local_menu(&mut self, title: &str, controls: &[&str]) -> bool {
        self.menu_with_status(title, controls, "MENU  /  LOCAL SESSION PAUSED")
    }
    /// [`GameShell::local_menu`] with a fourth entry, Settings: music/sound toggles and a "Save music"
    /// button (a drawn arrow, not a font glyph, so no font needs the glyph). The shell draws and reads
    /// clicks; it owns no audio state itself, so any game can use this without a dependency on
    /// `devkit::save` or `kit::audio` from this module.
    pub fn local_menu_with_audio(
        &mut self,
        title: &str,
        controls: &[&str],
        audio: AudioMenu,
    ) -> MenuOutcome {
        self.menu_with_status_and_audio(
            title,
            controls,
            "MENU  /  LOCAL SESSION PAUSED",
            audio,
            None,
            true,
        )
    }
    /// Stock audio settings, including online pause status. No export button without an export provider.
    pub fn menu_with_audio_settings(
        &mut self,
        title: &str,
        controls: &[&str],
        audio: AudioMenu,
        online: bool,
    ) -> MenuOutcome {
        self.menu_with_status_and_audio(
            title,
            controls,
            if online {
                "MENU  /  ONLINE MATCH CONTINUES"
            } else {
                "MENU  /  LOCAL SESSION PAUSED"
            },
            audio,
            None,
            false,
        )
    }
    /// [`GameShell::local_menu_with_audio`] for a game that supports shadows (`kit::Shadows`): the Settings
    /// screen gains a "Shadows: Off / Simple / Full" selector below the sound toggle. Pass the current tier;
    /// a click sets [`MenuOutcome::cycle_shadows`]. Games that call `local_menu_with_audio` keep exactly
    /// the Settings screen they had.
    pub fn local_menu_with_options(
        &mut self,
        title: &str,
        controls: &[&str],
        audio: AudioMenu,
        shadows: super::devkit::ShadowQuality,
    ) -> MenuOutcome {
        self.menu_with_status_and_audio(
            title,
            controls,
            "MENU  /  LOCAL SESSION PAUSED",
            audio,
            Some(shadows),
            true,
        )
    }
    fn menu_with_status_and_audio(
        &mut self,
        title: &str,
        controls: &[&str],
        status: &str,
        audio: AudioMenu,
        shadows: Option<super::devkit::ShadowQuality>,
        allow_download: bool,
    ) -> MenuOutcome {
        let mut outcome = MenuOutcome::default();
        if !self.paused {
            return outcome;
        }
        set_default_camera();
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0., 0., w, h, Color::from_rgba(8, 18, 26, 130));
        let scale = (w / 700.).min(h / 520.).min(1.0);
        let pw = 420. * scale;
        let extra_row = if self.settings_screen && shadows.is_some() {
            54.
        } else {
            0.
        };
        let ph = (if self.controls || self.settings_screen {
            360.
        } else {
            376.
        } + extra_row)
            * scale;
        let x = (w - pw) * 0.5;
        let y = (h - ph) * 0.5;
        let ink = Color::from_rgba(24, 43, 53, 255);
        draw_rectangle(x, y, pw, ph, Color::from_rgba(243, 242, 232, 250));
        draw_text(title, x + 28. * scale, y + 43. * scale, 29. * scale, ink);
        draw_text(status, x + 28. * scale, y + 68. * scale, 14. * scale, ink);
        if self.controls {
            for (i, line) in controls.iter().enumerate() {
                draw_text(
                    line,
                    x + 28. * scale,
                    y + (108. + i as f32 * 26.) * scale,
                    18. * scale,
                    ink,
                );
            }
            if self.button(
                "Back",
                x + 24. * scale,
                y + ph - 64. * scale,
                pw - 48. * scale,
                42. * scale,
                true,
            ) || self.actions.accept
            {
                self.controls = false;
                self.suppress = true;
            }
        } else if self.settings_screen {
            let rows: Vec<_> = settings_rows(audio.has_music, shadows.is_some())
                .into_iter()
                .filter(|row| allow_download || *row != SettingsRow::Download)
                .collect();
            self.selection = menu_selection(self.selection, self.actions, rows.len());
            for (i, row_kind) in rows.iter().enumerate() {
                let label = match row_kind {
                    SettingsRow::Music => {
                        format!("Music: {}", if audio.music_on { "On" } else { "Off" })
                    }
                    SettingsRow::Sound => {
                        format!("Sound: {}", if audio.sfx_on { "On" } else { "Off" })
                    }
                    SettingsRow::Shadows => format!("Shadows: {}", shadows.unwrap().label()),
                    SettingsRow::Download => "Save music (.wav)".into(),
                    SettingsRow::Back => "Back".into(),
                };
                let by = if *row_kind == SettingsRow::Back {
                    y + ph - 64. * scale
                } else {
                    y + (108. + i as f32 * 54.) * scale
                };
                let clicked = self.button(
                    &label,
                    x + 24. * scale,
                    by,
                    pw - 48. * scale,
                    42. * scale,
                    self.selection == i,
                );
                if clicked || (self.selection == i && self.actions.accept) {
                    self.apply_setting(*row_kind, &mut outcome);
                }
                if *row_kind == SettingsRow::Download {
                    draw_download_arrow(x + pw - 54. * scale, by + 21. * scale, 8. * scale);
                }
            }
        } else {
            if self.actions.next {
                self.selection = (self.selection + 1) % 4;
            }
            if self.actions.previous {
                self.selection = (self.selection + 3) % 4;
            }
            for (i, label) in ["Resume", "Controls", "Settings", "Quit game"]
                .iter()
                .enumerate()
            {
                let clicked = self.button(
                    label,
                    x + 24. * scale,
                    y + (94. + i as f32 * 57.) * scale,
                    pw - 48. * scale,
                    46. * scale,
                    self.selection == i,
                );
                if clicked || (self.selection == i && self.actions.accept) {
                    self.suppress = true;
                    match i {
                        0 => self.paused = false,
                        1 => self.controls = true,
                        2 => {
                            self.settings_screen = true;
                            self.selection = 0;
                        }
                        _ => outcome.quit = true,
                    }
                }
            }
        }
        outcome
    }
    fn apply_setting(&mut self, row: SettingsRow, outcome: &mut MenuOutcome) {
        self.suppress = true;
        match row {
            SettingsRow::Music => outcome.toggle_music = true,
            SettingsRow::Sound => outcome.toggle_sfx = true,
            SettingsRow::Shadows => outcome.cycle_shadows = true,
            SettingsRow::Download => outcome.download_music = true,
            SettingsRow::Back => {
                self.settings_screen = false;
                self.selection = 0;
            }
        }
    }
    fn menu_with_status(&mut self, title: &str, controls: &[&str], status: &str) -> bool {
        if !self.paused {
            return false;
        }
        set_default_camera();
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0., 0., w, h, Color::from_rgba(8, 18, 26, 130));
        let scale = (w / 700.).min(h / 520.).min(1.0);
        let pw = 420. * scale;
        let ph = if self.controls { 360. } else { 320. } * scale;
        let x = (w - pw) * 0.5;
        let y = (h - ph) * 0.5;
        let ink = Color::from_rgba(24, 43, 53, 255);
        draw_rectangle(x, y, pw, ph, Color::from_rgba(243, 242, 232, 250));
        draw_text(title, x + 28. * scale, y + 43. * scale, 29. * scale, ink);
        draw_text(status, x + 28. * scale, y + 68. * scale, 14. * scale, ink);
        if self.controls {
            for (i, line) in controls.iter().enumerate() {
                draw_text(
                    line,
                    x + 28. * scale,
                    y + (108. + i as f32 * 26.) * scale,
                    18. * scale,
                    ink,
                );
            }
            if self.button(
                "Back",
                x + 24. * scale,
                y + ph - 64. * scale,
                pw - 48. * scale,
                42. * scale,
                true,
            ) || self.actions.accept
            {
                self.controls = false;
                self.suppress = true;
            }
        } else {
            if self.actions.next {
                self.selection = (self.selection + 1) % 3;
            }
            if self.actions.previous {
                self.selection = (self.selection + 2) % 3;
            }
            for (i, label) in ["Resume", "Controls", "Quit game"].iter().enumerate() {
                let clicked = self.button(
                    label,
                    x + 24. * scale,
                    y + (94. + i as f32 * 57.) * scale,
                    pw - 48. * scale,
                    46. * scale,
                    self.selection == i,
                );
                if clicked || (self.selection == i && self.actions.accept) {
                    self.suppress = true;
                    match i {
                        0 => self.paused = false,
                        1 => self.controls = true,
                        _ => return true,
                    }
                }
            }
        }
        false
    }
    fn button(&self, text: &str, x: f32, y: f32, w: f32, h: f32, selected: bool) -> bool {
        let (mx, my) = mouse_position();
        let hover = Rect::new(x, y, w, h).contains(vec2(mx, my));
        draw_rectangle(
            x,
            y,
            w,
            h,
            if hover || selected {
                Color::from_rgba(29, 94, 108, 255)
            } else {
                Color::from_rgba(221, 226, 220, 255)
            },
        );
        draw_text(
            text,
            x + 16.,
            y + h * 0.65,
            h * 0.48,
            if hover || selected {
                WHITE
            } else {
                Color::from_rgba(24, 43, 53, 255)
            },
        );
        hover && is_mouse_button_pressed(MouseButton::Left)
    }
}

/// A small drawn download icon (a stem over a downward arrowhead) at `size` scale centred on
/// `(cx, cy)`: no font glyph needs to exist for it, so it renders identically on every platform.
fn draw_download_arrow(cx: f32, cy: f32, size: f32) {
    let ink = Color::from_rgba(24, 43, 53, 255);
    draw_line(cx, cy - size, cx, cy + size * 0.15, size * 0.3, ink);
    draw_triangle(
        vec2(cx - size * 0.75, cy),
        vec2(cx + size * 0.75, cy),
        vec2(cx, cy + size),
        ink,
    );
}

/// Both key layouts normalize diagonals in the shared movement controller.
pub fn movement_axes() -> (f32, f32) {
    let held = |a, b| {
        if is_key_down(a) || is_key_down(b) {
            1.
        } else {
            0.
        }
    };
    (
        held(KeyCode::W, KeyCode::Up) - held(KeyCode::S, KeyCode::Down),
        held(KeyCode::D, KeyCode::Right) - held(KeyCode::A, KeyCode::Left),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SettingsRow {
    Music,
    Sound,
    Shadows,
    Download,
    Back,
}
fn settings_rows(music: bool, shadows: bool) -> Vec<SettingsRow> {
    let mut rows = Vec::new();
    if music {
        rows.push(SettingsRow::Music);
    }
    rows.push(SettingsRow::Sound);
    if shadows {
        rows.push(SettingsRow::Shadows);
    }
    if music {
        rows.push(SettingsRow::Download);
    }
    rows.push(SettingsRow::Back);
    rows
}
fn menu_selection(current: usize, actions: ShellActions, count: usize) -> usize {
    let current = current % count;
    if actions.next {
        (current + 1) % count
    } else if actions.previous {
        (current + count - 1) % count
    } else {
        current
    }
}
#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn keyboard_settings_visit_only_available_rows_and_activate_without_closing() {
        for music in [false, true] {
            for shadows in [false, true] {
                let rows = settings_rows(music, shadows);
                assert_eq!(rows.contains(&SettingsRow::Music), music);
                assert_eq!(rows.contains(&SettingsRow::Download), music);
                assert_eq!(rows.contains(&SettingsRow::Shadows), shadows);
                let mut shell = GameShell::in_state(false, true, false);
                shell.settings_screen = true;
                for (i, row) in rows.iter().enumerate() {
                    assert_eq!(shell.selection, i);
                    let mut outcome = MenuOutcome::default();
                    shell.apply_setting(*row, &mut outcome);
                    match row {
                        SettingsRow::Music => assert!(outcome.toggle_music),
                        SettingsRow::Sound => assert!(outcome.toggle_sfx),
                        SettingsRow::Shadows => assert!(outcome.cycle_shadows),
                        SettingsRow::Download => assert!(outcome.download_music),
                        SettingsRow::Back => assert!(!shell.settings_screen),
                    }
                    if *row != SettingsRow::Back {
                        assert!(shell.settings_screen);
                        shell.selection = menu_selection(
                            shell.selection,
                            ShellActions {
                                next: true,
                                ..Default::default()
                            },
                            rows.len(),
                        );
                    }
                }
                assert_eq!(
                    menu_selection(
                        0,
                        ShellActions {
                            previous: true,
                            ..Default::default()
                        },
                        rows.len()
                    ),
                    rows.len() - 1
                );
                assert_eq!(
                    menu_selection(
                        rows.len() - 1,
                        ShellActions {
                            next: true,
                            ..Default::default()
                        },
                        rows.len()
                    ),
                    0
                );
            }
        }
    }
}
