//! Short, automatic encounters. No conversation trees interrupt the platforming.
pub const CALL_TO_ACTION: &str =
    "Vesper stole our Dawnheart. Ring eight bells. Bring our village dawn.";
pub const ENDING: &str = "Mote brings the Dawnheart home. The bells ring. Morning finds everyone.";
pub const BOSS_NAMES: [&str; 8] = [
    "DEWMANE",
    "CHIMEBACK",
    "BRACK",
    "RIMEHART",
    "SOLARCH",
    "TEMPEST",
    "BRIAR MATRON",
    "VESPER",
];
pub const BOSS_EPITHETS: [&str; 8] = [
    "The garden's bound protector",
    "The echo bell's stone sentinel",
    "The marsh pump keeper",
    "The orchard's frozen stag",
    "The sundial's glass lion",
    "The skyways' storm heron",
    "The woods' thornbound mother",
    "The Bell Regent",
];
pub const NPC_NAMES: [&str; 8] = [
    "GRANDMA WICK",
    "PIP THE MINER",
    "REED THE MILLER",
    "NELL THE GROWER",
    "TAVI THE MENDER",
    "KITE THE COURIER",
    "FERN THE GUIDE",
    "GRANDMA WICK",
];
pub const NPC_LINES: [[&str; 4]; 8] = [
    [
        "Vesper took our Dawnheart! Ring eight bells before our village freezes.",
        "The garden's creatures wear Vesper's night collars. Watch their steps.",
        "A little light can cross a big gap. We believe in you, Mote.",
        "Dewmane protects this garden. Break the collar when its charge ends!",
    ],
    [
        "The first bell warmed our windows! The echo bell waits below.",
        "Those crystal wings belong here. Vesper's cold has made them restless.",
        "Follow the river's light. The caverns still remember morning.",
        "Chimeback's landing shakes loose echoes. Leap, then strike its tired crown!",
    ],
    [
        "Our pumps stopped when Vesper silenced the Copper Bell. Help us turn them!",
        "Seed-shell beetles clear the reeds. A loose shell still knows its work.",
        "The mills need dawn as much as we do. You are bringing it back.",
        "Brack was our pump keeper. Dodge the seeds; its collar opens after a volley!",
    ],
    [
        "Three bells! A green shoot appeared in my frozen orchard this morning.",
        "The branches are slippery. Give your feet a little room to slow down.",
        "Vesper promised us forever. He meant a winter that would never end.",
        "Rimehart's collar freezes the floor. Jump the slide, then free our stag!",
    ],
    [
        "The sundial points toward home. I can almost feel the Dawnheart again.",
        "These glass steps were made to catch sunlight. Vesper stole that too.",
        "Even broken bridges can carry a brave little light.",
        "Solarch leaps with the sun. Wait for the glass crown to cool!",
    ],
    [
        "Five bells are calling across the sky! Your village can hear them now.",
        "The windmills carry seed and rain. Vesper bound their guardian in thunder.",
        "You have come so far, Mote. Let the moving branches meet your feet.",
        "Tempest sweeps low before landing. Clear its wings, then touch the collar!",
    ],
    [
        "Our lost grove glows again. One woodland bell, then the citadel.",
        "The thorns grew to shelter seedlings. Night collars turned them into walls.",
        "Vesper keeps the Dawnheart in his crown. Your home is still waiting.",
        "The Matron guards her young. Leap the thorn seeds and free their mother!",
    ],
    [
        "I followed the bells. We will carry the Dawnheart home together.",
        "These gears steal the warmth of every dawn. You can stop them.",
        "Seven bells, one little light. Our village kept a window bright for you.",
        "Vesper wears our Dawnheart! Let his fury pass, then break his crown!",
    ],
];
pub const BELL_LINES: [&str; 8] = [
    "The Dew Bell rings. Dewmane is free. A warm spark reaches home.",
    "The Echo Bell rings. Chimeback is free. Our village windows glow.",
    "The Copper Bell rings. Brack is free. The waterwheels turn again.",
    "The Frost Bell rings. Rimehart is free. The orchard begins to thaw.",
    "The Sun Bell rings. Solarch is free. Daylight touches the old ruins.",
    "The Storm Bell rings. Tempest is free. The winds carry hope home.",
    "The Dusk Bell rings. The Matron is free. The citadel stands alone.",
    "The Dawnheart is yours. The eighth bell calls Mote home!",
];
pub const BOSS_TELLS: [&str; 8] = [
    "Horns lowered - a root charge is coming",
    "Stone crown rising - leap the landing echoes",
    "Copper pods swelling - seeds are coming",
    "Frost gathering - jump the ice slide",
    "Glass crown blazing - two sun leaps",
    "Wings spreading - a low sweep is coming",
    "Thorns blooming - leap the seed bursts",
    "Crown tolling - leap, sweep, then seeds",
];
