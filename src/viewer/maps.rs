//! Map selection is independent of graphics; studio remains a regression fixture.
use super::room::Room;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapId {
    #[default]
    House,
    Studio,
}
pub const ROADMAP: [(&str, &str); 4] = [
    ("house", "Two-story suburban home and fenced backyard"),
    ("school", "School, cafeteria and gym — planned"),
    ("office", "Office, meeting rooms and bathroom — planned"),
    ("store", "Convenience store — planned"),
];
pub fn build(map: MapId) -> crate::Result<Room> {
    match map {
        MapId::House => super::house::build(),
        MapId::Studio => super::room::build(),
    }
}
