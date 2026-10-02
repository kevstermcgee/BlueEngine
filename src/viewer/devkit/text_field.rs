//! A single-line text box that is only logic: typing, pasting, caret movement, hold-to-repeat.
//!
//! Games used to hand-roll "push a char, pop on backspace", which cannot paste a server address and has no
//! caret. [`TextField`] is graphics-free and unit-testable; a game draws `text` and the caret itself.
//! With the `client` feature, `TextField::feed_frame` (in `viewer::game_input`) reads the keyboard and the
//! clipboard once per frame and calls the methods here.
//!
//! Every insertion is sanitised: control characters are removed, characters the [`CharFilter`] does not allow
//! are dropped, and the result is cut to `max_len` characters. A pasted block of several lines keeps its first
//! non-empty line (a trailing newline from copying a whole line is the usual case). `caret` and `max_len` count
//! characters, not bytes; use [`TextField::caret_byte`] to slice `text` for drawing.

/// Which characters a field accepts. Everything but `Any` is ASCII only (the engine's text atlas is ASCII).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CharFilter {
    /// Any non-control character.
    #[default]
    Any,
    /// A host name or IPv4 address with an optional port: letters, digits and `. : - _ [ ]`.
    Address,
    /// A player name: letters, digits, space, `'` and `-`.
    Name,
    /// Digits only (a port, a code).
    Digits,
}

impl CharFilter {
    /// Whether `c` may appear in a field with this filter. Control characters never may.
    pub fn allows(self, c: char) -> bool {
        if c.is_control() {
            return false;
        }
        match self {
            CharFilter::Any => true,
            CharFilter::Address => c.is_ascii_alphanumeric() || ".:-_[]".contains(c),
            CharFilter::Name => c.is_ascii_alphanumeric() || c == ' ' || c == '\'' || c == '-',
            CharFilter::Digits => c.is_ascii_digit(),
        }
    }
}

/// An editing key, independent of any window library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

/// Seconds a held editing key waits before it repeats, and between repeats.
pub const KEY_REPEAT_DELAY: f32 = 0.4;
pub const KEY_REPEAT_INTERVAL: f32 = 0.04;

#[derive(Clone, Debug, PartialEq)]
pub struct TextField {
    pub text: String,
    /// Insertion point as a character index, `0..=text.chars().count()`.
    pub caret: usize,
    /// Most characters the field holds.
    pub max_len: usize,
    pub filter: CharFilter,
    hold: Option<(EditKey, f32)>,
}

impl TextField {
    pub fn new(max_len: usize, filter: CharFilter) -> Self {
        Self {
            text: String::new(),
            caret: 0,
            max_len,
            filter,
            hold: None,
        }
    }

    /// A field that starts with `text` (sanitised like a paste), caret at the end.
    pub fn with_text(max_len: usize, filter: CharFilter, text: &str) -> Self {
        let mut field = Self::new(max_len, filter);
        field.insert_str(text);
        field
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
    /// Length in characters.
    pub fn len(&self) -> usize {
        self.text.chars().count()
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
    /// Byte offset of the caret in `text`, for `&text[..caret_byte()]` when drawing.
    pub fn caret_byte(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.caret)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// What `s` would contribute: first non-empty line only, filtered. (Not yet cut to the room left.)
    fn sanitised(&self, s: &str) -> Vec<char> {
        let line = s
            .lines()
            .map(|l| l.trim_matches(|c: char| c.is_control() || c == ' '))
            .find(|l| !l.is_empty())
            .unwrap_or("");
        line.chars().filter(|c| self.filter.allows(*c)).collect()
    }

    /// Type or paste `s` at the caret. Returns how many characters were actually inserted (0 when the field
    /// is full or nothing survived the filter). Never panics, whatever `s` holds.
    pub fn insert_str(&mut self, s: &str) -> usize {
        // A single typed character is not a "line": a lone space or tab must not be trimmed away.
        let chars = if s.chars().count() == 1 {
            s.chars().filter(|c| self.filter.allows(*c)).collect()
        } else {
            self.sanitised(s)
        };
        self.caret = self.caret.min(self.len());
        let room = self.max_len.saturating_sub(self.len());
        let inserted: String = chars.into_iter().take(room).collect();
        let count = inserted.chars().count();
        let at = self.caret_byte();
        self.text.insert_str(at, &inserted);
        self.caret += count;
        count
    }

    /// Type one character (what `get_char_pressed` yields).
    pub fn insert_char(&mut self, c: char) -> bool {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf)) == 1
    }

    /// Remove the character before the caret.
    pub fn backspace(&mut self) -> bool {
        self.caret = self.caret.min(self.len());
        if self.caret == 0 {
            return false;
        }
        self.caret -= 1;
        let at = self.caret_byte();
        self.text.remove(at);
        true
    }

    /// Remove the character after the caret.
    pub fn delete(&mut self) -> bool {
        self.caret = self.caret.min(self.len());
        if self.caret >= self.len() {
            return false;
        }
        let at = self.caret_byte();
        self.text.remove(at);
        true
    }

    pub fn move_left(&mut self) {
        self.caret = self.caret.min(self.len()).saturating_sub(1);
    }
    pub fn move_right(&mut self) {
        self.caret = (self.caret + 1).min(self.len());
    }
    pub fn home(&mut self) {
        self.caret = 0;
    }
    pub fn end(&mut self) {
        self.caret = self.len();
    }

    /// Empty the field.
    pub fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.hold = None;
    }

    /// Replace the whole text (sanitised like a paste), caret at the end.
    pub fn set_text(&mut self, text: &str) {
        self.clear();
        self.insert_str(text);
    }

    /// Do one editing key once.
    pub fn press(&mut self, key: EditKey) {
        match key {
            EditKey::Backspace => {
                self.backspace();
            }
            EditKey::Delete => {
                self.delete();
            }
            EditKey::Left => self.move_left(),
            EditKey::Right => self.move_right(),
            EditKey::Home => self.home(),
            EditKey::End => self.end(),
        }
    }

    /// Feed which editing key is held this frame (at most one; pass `None` when none) and the frame length.
    /// The key acts on the frame it goes down, again after [`KEY_REPEAT_DELAY`], then every
    /// [`KEY_REPEAT_INTERVAL`], so holding Backspace erases a long paste. Keyboard-library edge events do
    /// not repeat, which is why this tracks the hold itself.
    pub fn step_held(&mut self, held: Option<EditKey>, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        match (held, self.hold) {
            (None, _) => self.hold = None,
            (Some(key), Some((was, timer))) if key == was => {
                let mut timer = timer + dt;
                // Presses owed since the last frame; the cap stops a stalled frame from erasing everything.
                let mut presses = 0;
                while timer >= KEY_REPEAT_DELAY && presses < 4 {
                    timer -= KEY_REPEAT_INTERVAL;
                    presses += 1;
                    self.press(key);
                }
                self.hold = Some((key, timer));
            }
            (Some(key), _) => {
                self.press(key);
                self.hold = Some((key, 0.));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> TextField {
        TextField::new(40, CharFilter::Address)
    }

    #[test]
    fn typing_and_caret_edits_work_in_the_middle_of_the_text() {
        let mut f = address();
        f.insert_str("abcd");
        assert_eq!((f.as_str(), f.caret), ("abcd", 4));
        f.move_left();
        f.move_left();
        f.insert_char('X');
        assert_eq!((f.as_str(), f.caret), ("abXcd", 3));
        assert!(f.backspace());
        assert_eq!((f.as_str(), f.caret), ("abcd", 2));
        assert!(f.delete());
        assert_eq!((f.as_str(), f.caret), ("abd", 2));
        f.home();
        assert!(!f.backspace(), "nothing before the caret");
        f.end();
        assert!(!f.delete(), "nothing after the caret");
        f.move_right();
        assert_eq!(f.caret, 3, "the caret stops at the end");
        f.home();
        f.move_left();
        assert_eq!(f.caret, 0, "and at the start");
        f.clear();
        assert!(f.is_empty() && f.caret == 0);
    }

    #[test]
    fn a_paste_is_sanitised_filtered_and_truncated() {
        let mut f = TextField::new(12, CharFilter::Address);
        // Control characters and a stray space vanish; the rest survives.
        assert_eq!(f.insert_str("  my\u{7}host.example.com:27015\r\n"), 12);
        assert_eq!(f.as_str(), "myhost.examp", "cut to max_len");
        assert_eq!(f.caret, 12);
        assert_eq!(f.insert_str("more"), 0, "a full field takes nothing");

        let mut f = address();
        f.insert_str("\n\n  203.0.113.5:27015  \nsecond line\n");
        assert_eq!(
            f.as_str(),
            "203.0.113.5:27015",
            "a multi-line paste keeps its first non-empty line"
        );

        let mut f = address();
        f.insert_str("a b/c\\d?e=f#g");
        assert_eq!(
            f.as_str(),
            "abcdefg",
            "characters the filter refuses are dropped, not fatal"
        );

        let mut f = address();
        f.insert_str("\u{0}\u{1b}[31m\t\n");
        assert_eq!(
            f.as_str(),
            "[31m",
            "escape and NUL are removed; the printable rest is ordinary"
        );

        let mut f = TextField::new(5, CharFilter::Any);
        f.insert_str(&"é".repeat(100));
        assert_eq!(f.len(), 5);
        assert_eq!(f.text.len(), 10, "max_len counts characters, not bytes");
        f.move_left();
        f.backspace();
        assert_eq!(f.as_str(), "éééé");
        assert_eq!(&f.text[..f.caret_byte()], "ééé");
    }

    #[test]
    fn a_paste_lands_at_the_caret_and_only_the_room_left_is_used() {
        let mut f = TextField::new(10, CharFilter::Address);
        f.insert_str("ab.cd");
        f.move_left();
        f.move_left();
        assert_eq!(f.insert_str("1234567"), 5);
        assert_eq!(f.as_str(), "ab.12345cd");
        assert_eq!(f.caret, 8);
    }

    #[test]
    fn filters_accept_exactly_their_characters() {
        let n = TextField::with_text(20, CharFilter::Name, "  O'Brien-Smith 2!? \n");
        assert_eq!(
            n.as_str(),
            "O'Brien-Smith 2",
            "inner spaces stay, edges are trimmed"
        );
        let mut n = TextField::new(20, CharFilter::Name);
        assert!(
            n.insert_char(' ') && n.insert_char('x'),
            "a typed space is kept"
        );
        assert_eq!(n.as_str(), " x");
        let d = TextField::with_text(8, CharFilter::Digits, "27a0b1\u{661}5");
        assert_eq!(
            d.as_str(),
            "27015",
            "ASCII digits only, Arabic-Indic digits are refused"
        );
        let a = TextField::with_text(40, CharFilter::Address, "[::1]:80_x-y");
        assert_eq!(a.as_str(), "[::1]:80_x-y");
        let any = TextField::with_text(40, CharFilter::Any, "héllo wörld\t!");
        assert_eq!(any.as_str(), "héllo wörld!");
        assert!(!CharFilter::Any.allows('\n'));
    }

    #[test]
    fn edits_never_panic_on_a_stale_caret() {
        let mut f = TextField::with_text(10, CharFilter::Any, "abc");
        f.caret = 99; // a game that moved the public field
        f.insert_str("d");
        assert_eq!(f.as_str(), "abcd");
        f.caret = 99;
        assert!(f.backspace());
        f.caret = 99;
        assert!(!f.delete());
        f.set_text("zz");
        assert_eq!((f.as_str(), f.caret), ("zz", 2));
    }

    #[test]
    fn a_held_key_acts_once_then_repeats() {
        let mut f = TextField::with_text(100, CharFilter::Any, &"x".repeat(40));
        let dt = 1. / 60.;
        f.step_held(Some(EditKey::Backspace), dt);
        assert_eq!(f.len(), 39, "acts on the frame it goes down");
        for _ in 0..20 {
            f.step_held(Some(EditKey::Backspace), dt);
        }
        assert_eq!(f.len(), 39, "nothing during the initial delay (0.33 s)");
        for _ in 0..60 {
            f.step_held(Some(EditKey::Backspace), dt);
        }
        let erased = 40 - f.len();
        assert!(
            (20..=30).contains(&erased),
            "about 1/0.04 per second after the delay: {erased}"
        );
        f.step_held(None, dt);
        let before = f.len();
        f.step_held(Some(EditKey::Left), dt);
        assert_eq!(f.len(), before);
        assert_eq!(f.caret, before - 1, "a different key starts over");
        f.step_held(Some(EditKey::Left), 10.);
        assert!(
            f.caret >= before - 1 - 4,
            "a stalled frame owes a bounded number of repeats"
        );
    }

    #[test]
    fn non_finite_frame_times_do_not_poison_the_repeat() {
        let mut f = TextField::with_text(10, CharFilter::Any, "abc");
        f.step_held(Some(EditKey::Left), f32::NAN);
        f.step_held(Some(EditKey::Left), f32::INFINITY);
        assert!(f.caret <= 2);
    }
}
