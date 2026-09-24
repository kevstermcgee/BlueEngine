//! Runtime physics checks for the authored map extensions, independent of graphics.
use std::path::Path;
use vesper3d::{
    math::{Ray, V},
    viewer::{
        authoring::MapDocument,
        controller::{CharacterKind, Controller},
        prop_physics::PropPhysics,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/maps/starters".into());
    for name in ["house", "office", "school-wing", "convenience-store"] {
        let doc = MapDocument::load(&Path::new(&directory).join(format!("{name}.json")))?;
        for kind in [CharacterKind::Scientist, CharacterKind::Feta] {
            let mut room = doc.build()?;
            let mut physics = PropPhysics::new(&mut room)?;
            let mut player = Controller::for_character(kind);
            let ids: Vec<_> = physics
                .props
                .iter()
                .filter(|p| p.id.starts_with("expansion/"))
                .map(|p| p.id.clone())
                .collect();
            assert!(ids.len() >= 50, "{name}: too few physical expansion props");
            for _ in 0..600 {
                physics.advance(1.0 / 60.0, &player, &mut room);
            }
            for id in &ids {
                let e = room.entities.iter().find(|e| &e.id == id).unwrap();
                assert!(e.bounds.min.finite() && e.bounds.max.finite());
                assert!(e.bounds.min.1 > -0.1, "{name}: {id} fell through floor");
            }
            // Pick up a freestanding chair from the adjacent aisle, then release it.
            let id = match name {
                "house" => "expansion/workshop/table-0/chair",
                "office" => "expansion/training/desk-0-0/chair",
                "school-wing" => "expansion/art/desk-0/chair",
                _ => "expansion/picnic/table-0-0/chair",
            };
            let e = room.entities.iter().find(|e| e.id == id).unwrap();
            let center = (e.bounds.min + e.bounds.max) * 0.5;
            let eye = player.position.1;
            player.position = V(center.0 + 1.1, eye, center.2);
            // Aim at a solid leg rather than the empty space beneath the seat.
            let aim = V(
                e.bounds.max.0 - 0.04,
                e.bounds.min.1 + 0.16,
                e.bounds.max.2 - 0.04,
            );
            let direction = (aim - player.position).norm();
            let ray = Ray {
                o: player.position,
                d: direction,
            };
            assert!(
                physics.toggle(&room, ray),
                "{name} {kind:?}: could not pick up chair"
            );
            assert_eq!(physics.held().unwrap().id, id);
            for _ in 0..60 {
                physics.advance(1.0 / 60.0, &player, &mut room);
            }
            assert!(physics.toggle(&room, ray));
            assert!(physics.held().is_none());
            for _ in 0..300 {
                physics.advance(1.0 / 60.0, &player, &mut room);
            }
            println!(
                "{name} {kind:?}: {} expansion rigid bodies; settle, pickup and drop passed",
                ids.len()
            );
        }
    }
    Ok(())
}
