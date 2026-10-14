#!/usr/bin/env python3
"""Check selected native graphs in isolated external consumers, without feature unification.

Normal edges describe dependencies compiled for that consumer, not Cargo.lock membership.
Use --run to compile/run the public-API fixtures; the default only resolves graphs.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HEAVY = {'rapier3d', 'parry3d', 'quinn', 'quinn-proto', 'tokio', 'rustls'}
DEVICES = {'macroquad', 'miniquad', 'quad-snd', 'gilrs', 'gilrs-core'}
RULES = r'''
use vesper3d::runtime::{Simulation, Snapshot, snapshot};
#[derive(Default)]
struct Dash { score: u64 }
impl Simulation for Dash {
    type Input = bool;
    fn step(&mut self, dash: &bool) { self.score += if *dash { 3 } else { 1 }; }
    fn state_hash(&self) -> u64 { self.score }
}
impl Snapshot for Dash {
    const KIND: &'static str = "boundary-dash";
    type State = u64;
    fn capture(&self) -> u64 { self.score }
    fn restore(&mut self, score: u64) -> Result<(), String> { self.score = score; Ok(()) }
}
fn main() {
    let mut game = Dash::default();
    game.step(&true);
    assert_eq!(game.score, 3); // This rule belongs to the game.
    let saved = snapshot::save(&game, "dash").unwrap();
    game.step(&false);
    snapshot::restore(&mut game, &saved).unwrap();
    assert_eq!(game.score, 3);
    snapshot::assert_resumes_exactly(Dash::default, &[true, false, true, true], 2);
}
'''
TWO_D = r'''
fn main() {
    use vesper3d::two_d::{Body, Point, Rect};
    let mut body = Body { bounds: Rect::new(0, 0, 10, 10), velocity: Point::new(8, 0), grounded: false };
    body.step(1, &[Rect::new(20, 0, 1, 100)]);
    body.step(1, &[Rect::new(20, 0, 1, 100)]);
    assert_eq!(body.bounds.x, 10);
}
'''
PHYSICS = r'''
fn main() {
    use vesper3d::prelude::*;
    let mut world = SceneBuilder::new("Boundary world").without_spawn()
        .structural_box("floor", V(0., -0.1, 0.), V(8., 0.1, 8.), V::ONE)
        .prop("ball", "apple", V(2., 1., 0.)).world().unwrap();
    assert!(world.impulse("ball", V(0., 2., 0.)));
    world.step();
    let saved = world.save_state().unwrap();
    world.step();
    world.restore_state(&saved).unwrap();
    assert_eq!(world.checksum(), saved.checksum);
    let _: Option<vesper3d::rapier::prelude::RigidBodyHandle> = None;
}
'''
NETPLAY = r'''
fn main() {
    use vesper3d::viewer::{net::loopback::LoopNet, netplay::{NetServer, ServerConfig, toy::ToyGame}};
    let net = LoopNet::new(1, 0, 0., 7);
    let mut server = NetServer::<ToyGame, _>::new(net.endpoint("10.0.0.1:4000".parse().unwrap()), ServerConfig::default()).unwrap();
    server.step(std::time::Instant::now());
}
'''
SCHEMA = r'''
fn main() {
    let schema = vesper3d::authoring_schemas::generate("game").unwrap();
    assert_eq!(schema["title"], "BE2 GameDocument v1");
}
'''
PRESENTATION = r'''
fn main() {
    // Resolve custom controls/window APIs without opening a device in this headless fixture.
    let _input_factory = vesper3d::viewer::game_input::ClientInput::new;
    let _window_factory = vesper3d::viewer::game_client::window_config;
    let _look = vesper3d::viewer::devkit::FpsCamera::default();
}
'''
RESOURCES = r'''
use std::collections::BTreeMap;
use vesper3d::runtime::{resources::Inventory, Simulation, Snapshot, StateHasher, snapshot};
struct Craft { inventory: Inventory }
impl Simulation for Craft {
    type Input = bool;
    fn step(&mut self, craft: &bool) {
        if *craft {
            self.inventory.transact(&[("probe:ore".into(), 2)], &[("probe:plate".into(), 1)]).unwrap();
        }
    }
    fn state_hash(&self) -> u64 {
        let mut hash = StateHasher::new();
        hash.u64(self.inventory.max_units());
        for (id, capacity) in self.inventory.capacities() { hash.str(id).u64(*capacity); }
        for (id, count) in self.inventory.counts() { hash.str(id).u64(*count); }
        hash.finish()
    }
}
impl Snapshot for Craft {
    const KIND: &'static str = "boundary-resource-craft";
    type State = Inventory;
    fn capture(&self) -> Inventory { self.inventory.clone() }
    fn restore(&mut self, inventory: Inventory) -> Result<(), String> {
        inventory.validate().map_err(|error| error.to_string())?;
        self.inventory = inventory;
        Ok(())
    }
}
fn main() {
    let mut inventory = Inventory::new(8, BTreeMap::from([
        ("probe:ore".into(), 8), ("probe:plate".into(), 8),
    ])).unwrap();
    inventory.transact(&[], &[("probe:ore".into(), 4)]).unwrap(); // declared source
    let mut game = Craft { inventory };
    game.step(&true);
    assert_eq!((game.inventory.quantity("probe:ore"), game.inventory.quantity("probe:plate")), (2, 1));
    let saved = snapshot::save(&game, "in-flight craft").unwrap();
    game.step(&true);
    snapshot::restore(&mut game, &saved).unwrap();
    assert_eq!((game.inventory.quantity("probe:ore"), game.inventory.quantity("probe:plate")), (2, 1));
    let before = game.state_hash();
    assert!(game.inventory.transact(&[("probe:ore".into(), 3)], &[("probe:plate".into(), 1)]).is_err());
    assert_eq!(game.state_hash(), before);
}
'''
CASES = {
    'pure_rules': ([], set(), HEAVY | DEVICES, RULES),
    'resource_transactions': (['resources'], {'serde'}, HEAVY | DEVICES | {'alsa-sys'}, RESOURCES),
    'offline_2d': (['two-d'], {'macroquad'}, HEAVY, TWO_D),
    'spatial_3d': (['physics'], {'rapier3d', 'parry3d'}, {'quinn', 'tokio', 'rustls'} | DEVICES, PHYSICS),
    'native_multiplayer': (['networking'], {'quinn', 'tokio', 'rustls'}, {'rapier3d', 'parry3d'} | DEVICES, NETPLAY),
    'custom_presentation': (['native-client'], {'macroquad', 'gilrs'}, HEAVY, PRESENTATION),
    'authoring_schemas': (['schema-generation'], {'schemars'}, HEAVY | DEVICES, SCHEMA),
}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true', help='Also compile/run public-API fixtures')
    parser.add_argument('--target', help='Cargo target triple; defaults to rustc native host')
    parser.add_argument('--profile', choices=['itest', 'dev'], default='itest')
    args = parser.parse_args(argv)
    target = args.target or next(line.split(': ', 1)[1] for line in subprocess.check_output(
        ['rustc', '-Vv'], text=True).splitlines() if line.startswith('host: '))
    if args.run and args.target:
        parser.error('--run is native-host only; omit --target')
    receipts = {}
    with tempfile.TemporaryDirectory(prefix='be2-boundaries ') as temporary:
        for name, (features, required, forbidden, source) in CASES.items():
            project = Path(temporary) / name
            (project / 'src').mkdir(parents=True)
            (project / 'Cargo.toml').write_text(
                '[package]\nname = "boundary-' + name.replace('_', '-') + '"\nversion = "0.0.0"\nedition = "2021"\n'
                '[workspace]\n[dependencies]\nvesper3d = { package = "be2", path = ' + json.dumps(str(ROOT), ensure_ascii=False) +
                ', default-features = false, features = ' + json.dumps(features) + ' }\n'
                '[profile.itest]\ninherits = "dev"\ndebug = "line-tables-only"\n'
                '[profile.itest.package."*"]\nopt-level = 3\ndebug = false\n'
                '[profile.itest.package.be2]\nopt-level = 0\ndebug = "line-tables-only"\n', encoding='utf-8')
            (project / 'src/main.rs').write_text(source, encoding='utf-8')
            manifest = str(project / 'Cargo.toml')
            # Retain engine-tested dependency versions while adding this standalone
            # root package. generate-lockfile would choose unrelated newer versions.
            shutil.copyfile(ROOT / 'Cargo.lock', project / 'Cargo.lock')
            subprocess.run(['cargo', 'tree', '--offline', '--manifest-path', manifest, '--target', target],
                           check=True, capture_output=True, text=True)
            command = ['cargo', 'tree', '--locked', '--offline', '--manifest-path', manifest,
                       '--target', target, '--edges', 'normal', '--prefix', 'none', '--format', '{p}']
            lines = subprocess.check_output(command, text=True).splitlines()
            packages = {line.split()[0] for line in lines if line.strip()}
            unexpected, missing = sorted(packages & forbidden), sorted(required - packages)
            if unexpected or missing:
                raise SystemExit(f'{name}: dependency boundary violated; unexpected={unexpected}, missing={missing}. '
                                 'Inspect Cargo features for this external consumer; do not rely on workspace unification.')
            if args.run:
                subprocess.run(['cargo', 'run', '--locked', '--offline', '--profile', args.profile, '--manifest-path', manifest], check=True,
                               env={**os.environ, 'CARGO_TARGET_DIR': os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target'))})
            receipts[name] = {'target': target, 'features': features, 'normal_package_count': len(packages),
                              'packages': sorted(packages), 'fixture_executed': args.run, 'profile': args.profile if args.run else None}
    print(json.dumps({'boundary_checks': receipts}, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
