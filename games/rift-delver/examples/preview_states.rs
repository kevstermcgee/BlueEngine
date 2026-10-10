//! Create visual-review saves by playing ordinary intentions, never by replacing authority fields.
use rift_delver::{autopilot, Phase, Sim};
use vesper3d::viewer::devkit::{snapshot, SaveSlots};
fn main() {
    let dir = std::env::args().nth(1).expect("save directory");
    let slots = SaveSlots::new(dir);
    let mut sim = Sim::new(731);
    let mut saved = [false; 4];
    for _ in 0..300_000 {
        let phase = sim.s.phase;
        let i = match phase {
            Phase::Draft => Some(0),
            Phase::Gate => Some(1),
            Phase::Lost => Some(2),
            Phase::Combat if sim.s.depth == 3 && sim.s.wave == 3 => Some(3),
            _ => None,
        };
        if let Some(i) = i {
            if !saved[i] {
                snapshot::save_to_slot(
                    &sim,
                    &slots,
                    ["draft", "gate", "lost", "boss"][i],
                    "Visual review after ordinary play",
                )
                .unwrap();
                saved[i] = true;
            }
        }
        let mut intent = if saved[0] && saved[1] && saved[3] { rift_delver::Input::default() } else { autopilot(&sim) };
        if sim.s.phase == Phase::Camp {
            for i in 0..4 {
                if sim.s.meta.levels[i] < 4 && sim.s.meta.bank >= sim.cost(i) {
                    intent.choose = i as u8 + 1;
                    intent.interact = false;
                    break;
                }
            }
        }
        sim.step(&intent);
        sim.drain_events();
        if saved.iter().all(|b| *b) {
            break;
        }
    }
    assert!(
        saved.iter().all(|b| *b),
        "Preview states reached: {saved:?}, depth {}, phase {:?}, remaining {}",
        sim.s.depth,
        sim.s.phase,
        sim.remaining()
    );
    println!("Saved actual draft, gate, loss and depth-three boss states.");
}
