//! Reproducible real-world step and acknowledged replication measurements.
use std::{cell::RefCell, net::SocketAddr, time::Instant};
use vesper3d::{prelude::*, viewer::net::*};

#[derive(Default)]
struct Wire(RefCell<Vec<Vec<u8>>>);
impl DatagramTransport for Wire {
    fn send(&self, _: SocketAddr, bytes: &[u8]) -> vesper3d::Result<usize> {
        self.0.borrow_mut().push(bytes.to_vec());
        Ok(bytes.len())
    }
    fn receive(&mut self) -> vesper3d::Result<Vec<Datagram>> {
        Ok(vec![])
    }
    fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        Ok("127.0.0.1:4000".parse()?)
    }
}
fn report(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    println!(
        "{label},n={},p50={:.2},p95={:.2},p99={:.2}",
        samples.len(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100],
        samples[samples.len() * 99 / 100]
    );
}
fn world(count: usize, players: u64) -> HeadlessWorld {
    let mut scene = SceneBuilder::new("Performance").spawn(V(-4., 0., -4.), 0.);
    for i in 0..count {
        scene = scene.prop(
            format!("p{i:04}"),
            "apple",
            V((i % 32) as f32 * 0.7, 1., (i / 32) as f32 * 0.7),
        );
    }
    let mut world = scene.world().unwrap();
    for id in 1..=players {
        assert!(world.join(id));
    }
    for _ in 0..600 {
        world.step();
    }
    assert_eq!(world.prop_physics.as_ref().unwrap().props.len(), count);
    world
}
fn main() {
    for players in [2, 8] {
        for count in [32, 128, 512] {
            let mut w = world(count, players);
            for mode in ["settled", "one", "many", "resettled"] {
                if mode == "resettled" {
                    for _ in 0..600 {
                        w.step();
                    }
                }
                let mut samples = Vec::with_capacity(600);
                for t in 0..600 {
                    if t % 30 == 0 && (mode == "one" || mode == "many") {
                        for i in 0..if mode == "one" { 1 } else { count } {
                            w.impulse(&format!("p{i:04}"), V(0., 0.03, 0.));
                        }
                    }
                    let now = Instant::now();
                    w.step();
                    samples.push(now.elapsed().as_secs_f64() * 1e6);
                }
                report(
                    &format!("step_us,players={players},props={count},mode={mode}"),
                    samples,
                );
            }
            for o in &mut w.lifecycle.objects {
                o.state = vesper3d::viewer::lifecycle::LifecycleState::ReplicatedEntity;
            }
            let mut snap_times = vec![];
            for _ in 0..600 {
                let now = Instant::now();
                std::hint::black_box(w.snapshot(0));
                snap_times.push(now.elapsed().as_secs_f64() * 1e6);
            }
            report(
                &format!("snapshot_us,players={players},props={count}"),
                snap_times,
            );
            for rtt in [0, 50, 100, 200] {
                let wire = Wire::default();
                let peer = "127.0.0.1:4001".parse().unwrap();
                let mut sender = ReplicationSender::for_session([1, 2]);
                let mut receiver = None;
                let mut deliveries = vec![];
                let mut acks = vec![];
                let mut times = vec![];
                let mut ages = vec![];
                let mut desired = w.snapshot(0);
                for ms in 0..60_000u64 {
                    acks.retain(|&(due, tick)| {
                        if due <= ms {
                            sender.acknowledge(tick);
                            false
                        } else {
                            true
                        }
                    });
                    if ms % 50 == 0 {
                        for p in &mut desired.props {
                            p.position.0 = ms as f32;
                        }
                        for p in &mut desired.players {
                            p.position.0 = ms as f32;
                        }
                        desired.tick += 3;
                        let now = Instant::now();
                        sender.send(&wire, peer, &desired, 1).unwrap();
                        times.push(now.elapsed().as_secs_f64() * 1e6);
                        for bytes in wire.0.borrow_mut().drain(..) {
                            deliveries.push((ms + rtt / 2, bytes));
                        }
                    }
                    deliveries.retain(|(due, bytes)| {
                        if *due > ms {
                            return true;
                        }
                        receive_update(&mut receiver, Packet::decode(bytes).unwrap()).unwrap();
                        if let Some(b) = &receiver {
                            acks.push((ms + rtt / 2, b.tick));
                        }
                        false
                    });
                    if ms > 30_000 && ms % 50 == 0 {
                        if let Some(b) = &receiver {
                            for p in &b.props {
                                ages.push(ms as f64 - p.position.0 as f64);
                            }
                        }
                    }
                }
                report(
                    &format!("send_us,players={players},props={count},rtt={rtt}"),
                    times,
                );
                report(
                    &format!("entity_age_ms,players={players},props={count},rtt={rtt}"),
                    ages,
                );
                println!(
                    "retained_serialized_bytes={},retries={}",
                    sender.retained_payload_bytes(),
                    sender.counters.retries
                );
            }
        }
    }
}
