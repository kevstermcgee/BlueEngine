use crate::Cell;

pub struct Room {
    pub name: &'static str,
    pub lesson: &'static [&'static str],
    pub player: Cell,
    pub crates: &'static [Cell],
    pub docks: &'static [Cell],
    pub walls: &'static [Cell],
    pub pits: &'static [Cell],
    pub solution: &'static str,
}
const fn c(x: i32, y: i32) -> Cell {
    Cell { x, y }
}
pub const ROOMS: [Room; 6] = [
    Room {
        name: "A little gravity",
        lesson: &[
            "Every step leaves a vacuum.",
            "It pulls the first crate on",
            "each open ray by one tile.",
            "Fill both golden docks.",
        ],
        player: c(2, 4),
        crates: &[c(4, 2), c(4, 4)],
        docks: &[c(3, 1), c(3, 4)],
        walls: &[],
        pits: &[],
        solution: "UUURR",
    },
    Room {
        name: "Stand in the way",
        lesson: &[
            "Your new position blocks",
            "one ray. Use your body",
            "to shelter a crate.",
            "Docks never hold crates.",
        ],
        player: c(3, 4),
        crates: &[c(3, 2), c(5, 4)],
        docks: &[c(3, 2), c(4, 2)],
        walls: &[c(2, 2), c(2, 3)],
        pits: &[],
        solution: "URUULR",
    },
    Room {
        name: "One behind another",
        lesson: &[
            "Only the first crate on",
            "each ray feels the pull.",
            "Hide one crate behind",
            "another; move the shield.",
        ],
        player: c(2, 5),
        crates: &[c(4, 2), c(4, 3), c(2, 2)],
        docks: &[c(2, 1), c(2, 2), c(3, 2)],
        walls: &[c(3, 4)],
        pits: &[],
        solution: "UUDLUUURR",
    },
    Room {
        name: "Three-way conversation",
        lesson: &[
            "Every ray is read from",
            "the same board. Several",
            "crates can move at once.",
            "A pull never chains.",
        ],
        player: c(3, 3),
        crates: &[c(1, 3), c(3, 1), c(5, 3)],
        docks: &[c(2, 3), c(4, 1), c(5, 2)],
        walls: &[],
        pits: &[],
        solution: "URULRRDU",
    },
    Room {
        name: "The useful collision",
        lesson: &[
            "Two arrows, one square:",
            "both crates stay put.",
            "Make a deliberate jam",
            "to protect your work.",
        ],
        player: c(3, 5),
        crates: &[c(2, 2), c(4, 2), c(3, 1)],
        docks: &[c(2, 3), c(3, 2), c(4, 3)],
        walls: &[c(1, 2), c(5, 2)],
        pits: &[],
        solution: "UURLLR",
    },
    Room {
        name: "The edge of everything",
        lesson: &[
            "Crates can fall into pits.",
            "Preview before you step.",
            "A jam can save a crate;",
            "Z can save any mistake.",
        ],
        player: c(2, 4),
        crates: &[c(2, 2), c(4, 2), c(4, 4), c(3, 3)],
        docks: &[c(2, 4), c(2, 5), c(3, 3), c(4, 3)],
        walls: &[],
        pits: &[c(1, 2), c(5, 4), c(3, 1)],
        solution: "UDDRURLLDLU",
    },
];
