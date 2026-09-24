//! Map selection is independent of graphics; studio remains a regression fixture.
use super::room::Room;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapId {
    #[default]
    TestLab,
    House,
    Studio,
}
pub const ROADMAP: [(&str, &str); 5] = [
    (
        "test-lab",
        "Blue Test Lab: core engine validation environment",
    ),
    (
        "house",
        "Two-story suburban home and fenced backyard (reference)",
    ),
    ("school", "School, cafeteria and gym — legacy/planned"),
    (
        "office",
        "Office, meeting rooms and bathroom — legacy/planned",
    ),
    ("store", "Convenience store — legacy/planned"),
];
pub fn build(map: MapId) -> crate::Result<Room> {
    match map {
        MapId::TestLab => super::test_lab::build(),
        MapId::House => super::house::build(),
        MapId::Studio => super::room::build(),
    }
}
