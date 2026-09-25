//! Source-free Test Lab recipe: three one-shot switches unlock an exit terminal.
use super::{
    authoring::{write_new, Edit, MapDocument},
    game::*,
    maps::MapId,
    profile::ControllerProfile,
};
use crate::{math::V, Result};
use std::{collections::BTreeMap, path::Path};

pub fn documents() -> Result<(GameDocument, MapDocument)> {
    let map = MapDocument::from_map(MapId::TestLab)?;
    let mut edits = Vec::new();
    let ids = ["button-a", "button-b", "button-c", "exit"];
    for (i, id) in ids.iter().enumerate() {
        edits.push(Edit::AddBox {
            id: (*id).into(),
            label: if *id == "exit" {
                "Exit terminal".into()
            } else {
                format!("Switch {}", i + 1)
            },
            center: V(-3. + i as f32 * 2., 1.5, 1.),
            half_extents: V(0.3, 0.3, 0.3),
            color: if *id == "exit" {
                V(0.2, 0.85, 0.4)
            } else {
                V(0.2, 0.5, 0.95)
            },
        });
    }
    let map = map.apply(&edits)?;
    let mut rules = Vec::new();
    for id in &ids[..3] {
        rules.push(Rule {
            id: format!("press-{id}"),
            on_interact: Some((*id).into()),
            on_enter: None,
            on_exit: None,
            on_timer: None,
            condition: None,
            once: true,
            actions: vec![
                GameAction::Increment {
                    counter: "switches".into(),
                    amount: 1,
                },
                GameAction::SetEnabled {
                    entity: (*id).into(),
                    enabled: false,
                },
            ],
        });
    }
    rules.push(Rule {
        id: "unlock-exit".into(),
        on_interact: None,
        on_enter: None,
        on_exit: None,
        on_timer: None,
        condition: Some(Condition {
            counter: "switches".into(),
            equals: 3,
        }),
        once: true,
        actions: vec![GameAction::SetEnabled {
            entity: "exit".into(),
            enabled: true,
        }],
    });
    rules.push(Rule {
        id: "finish".into(),
        on_interact: Some("exit".into()),
        on_enter: None,
        on_exit: None,
        on_timer: None,
        condition: None,
        once: true,
        actions: vec![GameAction::Complete],
    });
    let game = GameDocument {
        schema_version: 1,
        name: "Three switches - Test Lab".into(),
        map: "map.json".into(),
        player_profile: ControllerProfile::default(),
        spawn_points: vec![
            SpawnPoint {
                id: "player-a".into(),
                feet: V(-3., 0., 3.),
                yaw: 0.,
            },
            SpawnPoint {
                id: "player-b".into(),
                feet: V(-1., 0., 3.),
                yaw: 0.,
            },
        ],
        counters: BTreeMap::from([("switches".into(), 0)]),
        interactables: ids
            .iter()
            .map(|id| Interactable {
                entity: (*id).into(),
                enabled: *id != "exit",
            })
            .collect(),
        trigger_zones: Vec::new(),
        movers: Vec::new(),
        timers: Vec::new(),
        rules,
    };
    game.validate(&map)?;
    Ok((game, map))
}

pub fn write(directory: &Path) -> Result<()> {
    let (game, map) = documents()?;
    let game_bytes = serde_json::to_vec_pretty(&game)?;
    let map_bytes = serde_json::to_vec_pretty(&map)?;
    std::fs::create_dir(directory)?;
    write_new(&directory.join("map.json"), &map_bytes)?;
    write_new(&directory.join("game.json"), &game_bytes)?;
    Ok(())
}
