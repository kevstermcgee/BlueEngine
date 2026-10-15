//! A curated emotional vocabulary, recomposed into singular living works.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exhibit {
    pub name: String,
    pub artist: String,
    pub story: String,
    pub medium: String,
    pub emotion: usize,
    pub seed: u64,
    pub frame: usize,
}
pub const THEMES: [&str; 8] = ["Wonder", "Longing", "Grief", "Joy", "Unease", "Anger", "Tenderness", "Acceptance"];
pub const MOTTO: [&str; 8] = [
    "The world has not finished becoming.",
    "Some distances live inside us.",
    "What is absent still has a shape.",
    "Even light needs someone to dance with.",
    "The familiar has begun to dream.",
    "Give the fire somewhere to go.",
    "To be held is to become less heavy.",
    "Nothing has to stay to have mattered.",
];
pub const PALETTES: [([f32; 3], [f32; 3]); 8] = [
    ([0.14, 0.63, 0.65], [1., 0.76, 0.38]),
    ([0.39, 0.42, 0.82], [0.97, 0.59, 0.64]),
    ([0.18, 0.34, 0.48], [0.72, 0.83, 0.87]),
    ([0.95, 0.47, 0.16], [0.98, 0.83, 0.35]),
    ([0.34, 0.55, 0.39], [0.85, 0.36, 0.55]),
    ([0.8, 0.18, 0.15], [1., 0.55, 0.19]),
    ([0.72, 0.4, 0.53], [0.98, 0.76, 0.62]),
    ([0.34, 0.6, 0.52], [0.84, 0.78, 0.57]),
];
const NAMES: [[&str; 6]; 8] = [
    [
        "The Sky That Grew Roots",
        "A Door in the Moon",
        "Small Infinity",
        "The Shape of Almost",
        "Borrowed Constellations",
        "What the Stars Remember",
    ],
    [
        "The Distance Between Two Suns",
        "A House Waiting for Rain",
        "Letters Never Sent",
        "An Embrace of Empty Air",
        "Where You Used to Stand",
        "The Last Train Home",
    ],
    [
        "The Ocean After Everyone Left",
        "A Tree Full of Goodbyes",
        "Rooms for the Missing",
        "The Weight of a Name",
        "An Unfinished Heart",
        "A Bell Without a Voice",
    ],
    [
        "The Sun Forgot to Set",
        "A Garden of First Times",
        "A Thousand Little Yeses",
        "The Laughing Orbit",
        "Permission to Bloom",
        "The Colour of Coming Home",
    ],
    [
        "Someone Moved the Horizon",
        "The Room Behind Your Eyes",
        "A Moon That Watches Back",
        "The Unfamiliar Twin",
        "A Promise With Teeth",
        "The Clock That Listens",
    ],
    [
        "All the Things I Swallowed",
        "A Storm Asking to Be Held",
        "The Last Red Sea",
        "Anatomy of a Shout",
        "The Fire That Learned to Dance",
        "A Choir of Broken Doors",
    ],
    [
        "A Light Left On For You",
        "The Smallest Safe Place",
        "How to Hold a Falling Star",
        "Two Hearts Learning Gravity",
        "The Warmth Between Words",
        "A Lullaby for the Unseen",
    ],
    [
        "The River Keeps No Names",
        "Let the Horizon Go",
        "The Beauty of Leaving",
        "A Stone That Finally Floats",
        "The Quiet After Forgiveness",
        "Everything Returns as Light",
    ],
];
const STORIES:[&str;8]=[
    "Made after the artist saw a seed take root in an abandoned observatory. Each drifting light is a question they refused to stop asking. The landscape grows toward anyone who stands still long enough.",
    "Created during twelve winters of waiting for a letter. The artist painted the places between a departure and a return. The horizon moves away when approached, but its smallest frame always lets you closer.",
    "The artist could no longer remember a loved one's voice, so they built a place for its silence. Nothing here is dead. Every falling fragment is trying, gently, to become something that can be carried.",
    "On the first morning after a long illness, the artist decided joy should have no permission slips. They built a world where impossible things celebrate their own existence. Its rhythm changes when someone arrives.",
    "Found in a sealed apartment with no doors on its plans. An anonymous maker had recorded the same dream for forty years. The work recognises its visitors, although nobody has explained how.",
    "Built from the imagined remains of every word the artist was told to keep quiet. It burns without consuming itself. They called it a place where anger could become motion instead of harm.",
    "A gift for someone who believed they took up too much space. The artist gave every small light a shelter, and every shelter a song. Approach softly: the work is learning how to trust.",
    "The artist returned to a river they had tried to paint as a child. This time they let the water finish the picture. The work offers no answer, only the feeling that an ending can also be a place to rest."];
const ARTISTS: [&str; 16] = [
    "Mira Solenne",
    "Ivo Noct",
    "Anya Vale",
    "Sora Mendel",
    "Luca Ash",
    "Nadi Orin",
    "Anonymous",
    "Elian Moss",
    "Faye Aster",
    "Rin Bellweather",
    "Tomas Lune",
    "Celia Voss",
    "Jun Merrow",
    "Amara Still",
    "Ori Sable",
    "The Unknown Cartographer",
];
const PLACES: [&str; 12] = [
    "a city made of rain",
    "the island of lost afternoons",
    "a forgotten coastal station",
    "a room above the clouds",
    "the night garden of Vey",
    "a valley with no echo",
    "the last house in Bellweather",
    "an observatory beneath the sea",
    "a train crossing the winter sun",
    "the old harbour of Aster",
    "a library of unfinished dreams",
    "a village that only appears at dusk",
];
pub fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
pub fn room_seed(seed: u64, path: &[u8]) -> u64 {
    path.iter().fold(mix(seed), |s, b| mix(s ^ (*b as u64 + 37)))
}
pub fn theme(seed: u64) -> usize {
    (seed.rotate_right(17) % 8) as usize
}
pub fn exhibit(room: u64, slot: usize, depth: usize) -> Exhibit {
    let seed = mix(room ^ (slot as u64).wrapping_mul(0x9e3779b9) ^ (depth as u64).wrapping_mul(701));
    let e = theme(room);
    let index = (slot + (seed % 3) as usize) % 6;
    let medium = match slot {
        0..=2 => "Living pigment / traversable memory",
        3 => "Suspended bronze / breathing form",
        4 => "Kinetic glass / impossible gravity",
        _ => "Light, thread and an unheard choir",
    };
    Exhibit {
        name: format!("{} / Study {}", NAMES[e][index], seed % 10000),
        artist: ARTISTS[(seed.rotate_right(8) % 16) as usize].into(),
        story: format!(
            "{} Begun in {}. This is the only version that remembers this room.",
            STORIES[e],
            PLACES[(seed.rotate_right(25) % 12) as usize]
        ),
        medium: medium.into(),
        emotion: e,
        seed,
        frame: ((room.rotate_left(5) % 5) as usize + slot) % 5,
    }
}
