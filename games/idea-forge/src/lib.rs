//! Mechanic-first idea generation. Rules, history and persistence are rendering-free.
use serde::{Deserialize, Serialize};
use vesper3d::two_d::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub title: String,
    pub genre: String,
    pub mechanic: String,
    #[serde(rename = "loop")]
    pub loop_: String,
    pub fun: String,
    pub prototype: String,
    pub risk: String,
    pub story: String,
}

pub fn catalog() -> Vec<Idea> {
    // The authored field is named `loop`; serde keeps that public export contract.
    serde_json::from_str(include_str!("../assets/ideas.json")).expect("validated idea catalog")
}

pub const GENRES: [&str; 4] = ["all", "puzzle", "action", "strategy"];
pub const BUTTONS: [Rect; 13] = [
    Rect::new(184, 352, 142, 30), // generate
    Rect::new(334, 352, 120, 30), // favorite
    Rect::new(462, 352, 62, 30),  // previous
    Rect::new(532, 352, 62, 30),  // next
    Rect::new(602, 352, 170, 30), // library
    Rect::new(184, 65, 90, 28),
    Rect::new(282, 65, 142, 28),
    Rect::new(432, 65, 142, 28),
    Rect::new(582, 65, 190, 28),
    Rect::new(28, 122, 134, 38),
    Rect::new(28, 173, 134, 38),
    Rect::new(28, 224, 134, 38),
    Rect::new(28, 287, 134, 38),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub tick: u32,
    pub seed: u64,
    pub order: Vec<usize>,
    pub generated: Vec<usize>,
    pub favorites: Vec<usize>,
    pub current: usize,
    pub genre: usize,
    pub saved_only: bool,
    pub story: bool,
    pub page: usize,
    pub focus: usize,
    pub nav: [i32; 2],
    pub repeat: u32,
    pub exhausted: bool,
}

pub struct Forge {
    pub state: State,
    pub ideas: Vec<Idea>,
    cues: Vec<usize>,
    // Verification observation only; never part of gameplay or its state hash.
    probe_baseline: usize,
}

impl Forge {
    pub fn visible(&self) -> Vec<usize> {
        let list = if self.state.saved_only {
            &self.state.favorites
        } else {
            &self.state.generated
        };
        list.iter()
            .copied()
            .filter(|&id| self.matches(id))
            .collect()
    }

    fn matches(&self, id: usize) -> bool {
        self.state.genre == 0 || self.ideas[id].genre == GENRES[self.state.genre]
    }

    pub fn generate(&mut self) -> bool {
        let next = self
            .state
            .order
            .iter()
            .copied()
            .find(|id| self.matches(*id) && !self.state.generated.contains(id));
        self.state.exhausted = next.is_none();
        if let Some(id) = next {
            self.state.saved_only = false;
            self.state.generated.push(id);
            self.state.current = id;
            self.cues.push(0);
            true
        } else {
            false
        }
    }

    pub fn favorite(&mut self) {
        if !self.visible().contains(&self.state.current) {
            return;
        }
        if self.state.favorites.contains(&self.state.current) {
            self.state.favorites.retain(|id| *id != self.state.current);
        } else {
            self.state.favorites.push(self.state.current);
            self.cues.push(2);
        }
        self.select_visible();
    }

    fn select_visible(&mut self) {
        let list = self.visible();
        if !list.contains(&self.state.current) {
            if let Some(&id) = list.last() {
                self.state.current = id;
            }
        }
    }

    fn browse(&mut self, direction: i32) {
        let list = self.visible();
        if let Some(position) = list.iter().position(|id| *id == self.state.current) {
            let next = (position as i32 + direction).rem_euclid(list.len() as i32);
            self.state.current = list[next as usize];
        }
    }

    fn activate(&mut self, button: usize) {
        self.state.exhausted = false;
        match button {
            0 => {
                self.generate();
            }
            1 => self.favorite(),
            2 => self.browse(-1),
            3 => self.browse(1),
            4 => {
                self.state.saved_only = !self.state.saved_only;
                self.select_visible();
            }
            5..=8 => {
                self.state.genre = button - 5;
                self.select_visible();
                if self.visible().is_empty() && !self.state.saved_only {
                    self.generate();
                }
            }
            9..=11 => self.state.page = button - 9,
            12 => self.state.story = !self.state.story,
            _ => {}
        }
    }
}

impl GameLogic for Forge {
    const ID: &'static str = "idea-forge";
    const TITLE: &'static str = "Idea Forge";
    const CONTROLS: &'static str =
        "Click buttons | Arrows/pad select | Space/A activate | K save, L load";
    const VERIFY_TICKS: u32 = 180;
    fn new(seed: u64) -> Self {
        let ideas = catalog();
        let mut order: Vec<_> = (0..ideas.len()).collect();
        let mut rng = vesper3d::runtime::Rng::new(seed);
        for n in (1..order.len()).rev() {
            let other = rng.below(n + 1);
            order.swap(n, other);
        }
        let mut forge = Self {
            state: State {
                tick: 0,
                seed,
                order,
                generated: vec![],
                favorites: vec![],
                current: 0,
                genre: 0,
                saved_only: false,
                story: false,
                page: 0,
                focus: 0,
                nav: [0, 0],
                repeat: 0,
                exhausted: false,
            },
            ideas,
            cues: vec![],
            probe_baseline: 1,
        };
        forge.generate();
        forge.cues.clear();
        forge
    }
    fn tick(&self) -> u32 {
        self.state.tick
    }
    fn outcome(&self) -> &'static str {
        "playing"
    }
    fn verification_input(tick: u32) -> Intent {
        let button = match tick {
            1 | 10 | 20 => Some(0),
            30 => Some(1),
            40 => Some(6),
            50 => Some(0),
            60 => Some(10),
            70 => Some(12),
            80 => Some(11),
            90 => Some(5),
            100 => Some(4),
            110 => Some(4),
            120 => Some(2),
            _ => None,
        };
        button
            .map(|b| Intent {
                pointer: Some(Point::new(BUTTONS[b].x + 10, BUTTONS[b].y + 10)),
                action: true,
                ..Default::default()
            })
            .unwrap_or_default()
    }
    fn probe_input() -> Intent {
        Intent {
            pointer: Some(Point::new(220, 365)),
            action: true,
            ..Default::default()
        }
    }
    fn probe_success(&self) -> bool {
        self.state.generated.len() + self.state.favorites.len() > self.probe_baseline
    }
    fn pointer_target_only_on_press() -> bool {
        true
    }
    fn cue_point(&self, cue: usize) -> Point {
        if cue == 2 {
            Point::new(394, 366)
        } else {
            Point::new(254, 366)
        }
    }
    fn take_cues(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.cues)
    }
    fn restart(&mut self) {
        // Restart is another creative prompt, never destructive to the library.
        self.state.focus = 0;
        self.state.nav = [0, 0];
        self.state.repeat = 0;
        self.state.genre = 0;
        self.state.saved_only = false;
        self.generate();
        self.probe_baseline = self.state.generated.len() + self.state.favorites.len();
    }
}

impl Simulation for Forge {
    type Input = Intent;
    fn step(&mut self, input: &Intent) {
        self.state.tick = self.state.tick.wrapping_add(1);
        self.state.repeat = self.state.repeat.saturating_sub(1);
        let nav = [input.x.clamp(-1, 1), input.y.clamp(-1, 1)];
        if nav != [0, 0] && (nav != self.state.nav || self.state.repeat == 0) {
            let delta = if nav[0] != 0 { nav[0] } else { nav[1] * 4 };
            self.state.focus =
                (self.state.focus as i32 + delta).rem_euclid(BUTTONS.len() as i32) as usize;
            self.state.repeat = 18;
        }
        self.state.nav = nav;
        if input.action {
            // A pointer outside a button does nothing; keyboard/gamepad uses focus.
            if let Some(point) = input.pointer {
                if let Some(button) = BUTTONS.iter().position(|rect| rect.contains(point)) {
                    self.state.focus = button;
                    self.activate(button);
                }
            } else {
                self.activate(self.state.focus);
            }
        }
    }
    fn state_hash(&self) -> u64 {
        hash_json(&self.state)
    }
}

impl Snapshot for Forge {
    const KIND: &'static str = "idea-forge";
    type State = State;
    fn capture(&self) -> State {
        self.state.clone()
    }
    fn restore(&mut self, state: State) -> Result<(), String> {
        let unique_valid = |ids: &[usize]| {
            let mut set = std::collections::BTreeSet::new();
            ids.iter()
                .all(|id| *id < self.ideas.len() && set.insert(*id))
        };
        if state.order.len() != self.ideas.len()
            || !unique_valid(&state.order)
            || state.generated.is_empty()
            || !unique_valid(&state.generated)
            || !unique_valid(&state.favorites)
            || !state.generated.contains(&state.current)
            || state
                .favorites
                .iter()
                .any(|id| !state.generated.contains(id))
            || state.genre >= GENRES.len()
            || state.page > 2
            || state.focus >= BUTTONS.len()
            || state.repeat > 18
            || state.nav.iter().any(|n| !(-1..=1).contains(n))
        {
            return Err("Invalid Idea Forge library; existing library was preserved".into());
        }
        self.state = state;
        self.probe_baseline = self.state.generated.len() + self.state.favorites.len();
        self.cues.clear();
        Ok(())
    }
}

/// Standalone exports deliberately omit story unless the author requests it.
pub fn markdown(ideas: &[Idea], story: bool) -> String {
    let mut text = String::from("# Idea Forge\n\nDistinct mechanic seeds; review originality against existing games before production.\n\n");
    for idea in ideas {
        text.push_str(&format!("## {}\n\nGenre: {} | ID: {}\n\n**Mechanic:** {}\n\n**Play loop:** {}\n\n**Why it could be fun:** {}\n\n**First prototype:** {}\n\n**Design risk:** {}\n\n",
            idea.title, idea.genre, idea.id, idea.mechanic, idea.loop_, idea.fun, idea.prototype, idea.risk));
        if story {
            text.push_str(&format!("**Optional story:** {}\n\n", idea.story));
        }
    }
    text
}

#[cfg(feature = "client")]
mod presentation {
    use super::*;
    use draw::{Color, Scene};
    const INK: Color = Color::new(0.055, 0.075, 0.13, 1.);
    const PANEL: Color = Color::new(0.10, 0.13, 0.20, 1.);
    const MUTED: Color = Color::new(0.66, 0.74, 0.83, 1.);
    const BRIGHT: Color = Color::new(0.91, 0.94, 0.97, 1.);
    const MINT: Color = Color::new(0.43, 0.91, 0.73, 1.);
    const GOLD: Color = Color::new(1., 0.75, 0.35, 1.);

    pub fn wrap(text: &str, width: usize) -> Vec<String> {
        let mut lines = vec![];
        let mut line = String::new();
        for word in text.split_whitespace() {
            if !line.is_empty() && line.len() + 1 + word.len() > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
    fn paragraph(scene: &mut Scene, text: &str, y: i32) {
        for (line, text) in wrap(text, 61).into_iter().enumerate() {
            scene.text(3, text, Point::new(202, y + line as i32 * 18), 18., BRIGHT);
        }
    }
    fn label(scene: &mut Scene, text: &str, y: i32) {
        scene.text(3, text, Point::new(202, y), 14., MINT);
    }
    impl draw::Game for Forge {
        fn show_hud() -> bool {
            false
        }
        fn menu_status(&self) -> String {
            "Generate a rule. Find the fun. Keep a favorite.".into()
        }
        fn draw(&self, scene: &mut Scene) {
            scene.rect(-10, Rect::new(0, 0, 800, 450), INK);
            scene.rect(-9, Rect::new(0, 0, 800, 3), MINT);
            scene.text(3, "IDEA FORGE", Point::new(28, 32), 30., BRIGHT);
            scene.text(
                3,
                "Start with a rule worth playing.",
                Point::new(28, 52),
                16.,
                MUTED,
            );
            scene.text(
                3,
                "BLUEENGINE  /  CREATIVE TOOLS",
                Point::new(482, 31),
                14.,
                MINT,
            );
            scene.rect(0, Rect::new(184, 105, 588, 241), PANEL);
            let visible = self.visible();
            if visible.contains(&self.state.current) {
                let idea = &self.ideas[self.state.current];
                scene.text(3, &idea.title, Point::new(202, 132), 27., BRIGHT);
                scene.text(
                    3,
                    format!(
                        "{}  /  {}{}",
                        idea.genre.to_uppercase(),
                        idea.id,
                        if self.state.favorites.contains(&self.state.current) {
                            "  /  KEPT"
                        } else {
                            ""
                        }
                    ),
                    Point::new(202, 151),
                    13.,
                    GOLD,
                );
                match self.state.page {
                    0 => {
                        label(scene, "THE GAMEPLAY MECHANIC", 174);
                        paragraph(scene, &idea.mechanic, 194);
                        label(scene, "THE PLAY LOOP", 294);
                        paragraph(scene, &idea.loop_, 314);
                    }
                    1 => {
                        label(scene, "WHY IT COULD BE FUN", 174);
                        paragraph(scene, &idea.fun, 194);
                        label(scene, "BUILD THIS FIRST", 230);
                        paragraph(scene, &idea.prototype, 250);
                        label(scene, "TEST THIS RISK", 306);
                        paragraph(scene, &idea.risk, 326);
                    }
                    _ => {
                        label(scene, "OPTIONAL STORY", 174);
                        paragraph(
                            scene,
                            if self.state.story {
                                &idea.story
                            } else {
                                "Story is off. The mechanic stands on its own. Enable Story to see an optional narrative premise."
                            },
                            194,
                        );
                        label(scene, "PLAYTEST QUESTION", 268);
                        paragraph(scene, "Can a player discover a surprising use of the rule in two minutes? Try the smallest prototype before adding content.", 288);
                    }
                }
            } else {
                scene.text(
                    3,
                    if self.state.saved_only {
                        "Your kept ideas go here."
                    } else {
                        "A fresh direction awaits."
                    },
                    Point::new(202, 165),
                    26.,
                    BRIGHT,
                );
                paragraph(
                    scene,
                    if self.state.saved_only {
                        "Keep an idea with the Keep button. Change the genre filter or return to History to find your next favorite."
                    } else {
                        "Choose Generate to create an unseen mechanic in this genre."
                    },
                    204,
                );
            }
            let labels = [
                "GENERATE",
                if self.state.favorites.contains(&self.state.current) {
                    "UNKEEP"
                } else {
                    "KEEP IDEA"
                },
                "PREV",
                "NEXT",
                if self.state.saved_only {
                    "VIEW HISTORY"
                } else {
                    "VIEW KEPT"
                },
                "ALL",
                "PUZZLE",
                "ACTION",
                "STRATEGY",
                "01  MECHANIC",
                "02  BUILD",
                "03  STORY",
                if self.state.story {
                    "STORY: ON"
                } else {
                    "STORY: OFF"
                },
            ];
            for (index, rect) in BUTTONS.iter().copied().enumerate() {
                let selected = index == 5 + self.state.genre || index == 9 + self.state.page;
                if index == self.state.focus {
                    scene.rect(
                        1,
                        Rect::new(rect.x - 2, rect.y - 2, rect.w + 4, rect.h + 4),
                        GOLD,
                    );
                }
                scene.rect(2, rect, if index == 0 || selected { MINT } else { PANEL });
                scene.text(
                    3,
                    labels[index],
                    Point::new(rect.x + 10, rect.y + rect.h / 2 + 5),
                    16.,
                    if index == 0 || selected { INK } else { BRIGHT },
                );
            }
            scene.text(3, "THE RULE COMES FIRST", Point::new(28, 112), 11., MUTED);
            scene.text(3, "36 distinct mechanics", Point::new(28, 354), 12., MUTED);
            scene.text(3, "No accounts. No API.", Point::new(28, 373), 12., MUTED);
            let position = visible
                .iter()
                .position(|id| *id == self.state.current)
                .map_or(0, |n| n + 1);
            scene.text(3, if self.state.exhausted {
                "No unseen ideas in this genre. Browse history or try another genre.".into()
            } else {
                format!("{} {} / {}   |   Generated {} / {}   |   Kept {}   |   Autosaved on this device",
                    if self.state.saved_only { "Kept" } else { "History" }, position, visible.len(),
                    self.state.generated.len(), self.ideas.len(), self.state.favorites.len())
            }, Point::new(28, 417), 15., MUTED);
            scene.text(3, "Click/tap  |  Arrows/pad select  |  Space/A activate  |  K save  L load  Esc pause", Point::new(28, 440), 15., MUTED);
        }
    }
    #[cfg(test)]
    #[test]
    fn all_authored_text_fits_reserved_rows() {
        for idea in catalog() {
            assert!(wrap(&idea.mechanic, 61).len() <= 5, "{} mechanic", idea.id);
            assert!(wrap(&idea.loop_, 61).len() <= 2, "{} loop", idea.id);
            for text in [&idea.fun, &idea.prototype, &idea.risk, &idea.story] {
                assert!(wrap(text, 61).len() <= 2, "{} paragraph", idea.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_distinct_rules_and_complete_briefs() {
        let ideas = catalog();
        assert_eq!(ideas.len(), 36);
        let mut ids = std::collections::BTreeSet::new();
        let mut mechanics = std::collections::BTreeSet::new();
        for idea in ideas {
            assert!(ids.insert(idea.id));
            assert!(mechanics.insert(idea.mechanic.to_lowercase()));
            assert!(GENRES[1..].contains(&idea.genre.as_str()));
            for text in [
                &idea.title,
                &idea.loop_,
                &idea.fun,
                &idea.prototype,
                &idea.risk,
                &idea.story,
            ] {
                assert!(!text.trim().is_empty());
                assert!(text.is_ascii());
            }
        }
    }
    #[test]
    fn generation_never_repeats_and_exhaustion_preserves_library() {
        let mut forge = Forge::new(0);
        for _ in 1..36 {
            assert!(forge.generate());
        }
        let library = forge.state.generated.clone();
        for _ in 0..50 {
            assert!(!forge.generate());
        }
        assert_eq!(forge.state.generated, library);
        assert_eq!(
            library
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            36
        );
        assert_ne!(Forge::new(1).state.order, Forge::new(2).state.order);
    }
    #[test]
    fn filter_favorites_history_and_restart_are_safe() {
        let mut forge = Forge::new(7);
        forge.favorite();
        let kept = forge.state.current;
        forge.activate(6);
        while forge.generate() {}
        assert_eq!(
            forge
                .state
                .generated
                .iter()
                .filter(|id| forge.ideas[**id].genre == "puzzle")
                .count(),
            12
        );
        forge.activate(4);
        forge.activate(5);
        assert_eq!(forge.state.current, kept);
        forge.browse(1);
        assert_eq!(forge.state.current, kept);
        forge.favorite();
        assert!(forge.visible().is_empty());
        forge.browse(-1); // empty library must never divide by zero
        forge.activate(4);
        forge.favorite();
        let favorites = forge.state.favorites.clone();
        let count = forge.state.generated.len();
        forge.restart();
        assert_eq!(forge.state.favorites, favorites);
        assert!(forge.state.generated.len() >= count);
    }
    #[test]
    fn public_route_is_deterministic_and_snapshot_resumes() {
        let inputs: Vec<_> = (0..Forge::VERIFY_TICKS)
            .map(Forge::verification_input)
            .collect();
        vesper3d::runtime::assert_deterministic(|| Forge::new(7), &inputs);
        vesper3d::runtime::snapshot::assert_resumes_exactly(|| Forge::new(7), &inputs, 75);
        let mut forge = Forge::new(7);
        for input in &inputs {
            forge.step(input);
        }
        assert!(forge.state.generated.len() >= 5);
        assert_eq!(forge.state.favorites.len(), 1);
        assert!(forge.state.story);
        assert_eq!(forge.state.page, 2);
        let (hash, outcome) = verify::<Forge>();
        assert_eq!(outcome, "playing");
        if let Ok(path) = std::env::var("BE2_VERIFY_REPORT") {
            std::fs::write(path, serde_json::json!({"hash":format!("{hash:016x}"), "outcome":outcome,
                "ticks":Forge::VERIFY_TICKS,"purpose":"Generate distinct ideas, filter genres, keep and revisit a favorite, and enable optional story through public buttons."}).to_string()).unwrap();
        }
    }
    #[test]
    fn malformed_restore_keeps_the_previous_library() {
        let mut forge = Forge::new(7);
        let hash = forge.state_hash();
        let mut state = forge.capture();
        state.order[1] = state.order[0];
        assert!(forge.restore(state).is_err());
        assert_eq!(forge.state_hash(), hash);
        let mut state = forge.capture();
        state.favorites.push(999);
        assert!(forge.restore(state).is_err());
        assert_eq!(forge.state_hash(), hash);
    }
    #[test]
    fn pointer_misses_are_ignored_and_keyboard_can_keep_an_idea() {
        let mut forge = Forge::new(7);
        forge.step(&Intent {
            action: true,
            pointer: Some(Point::new(780, 200)),
            ..Default::default()
        });
        assert_eq!(forge.state.generated.len(), 1);
        forge.step(&Intent {
            x: 1,
            action: true,
            ..Default::default()
        });
        assert!(forge.probe_success());
        assert_eq!(forge.state.favorites.len(), 1);
        assert!(!markdown(&forge.ideas[..1], false).contains("Optional story"));
        assert!(markdown(&forge.ideas[..1], true).contains("Optional story"));
    }
}
