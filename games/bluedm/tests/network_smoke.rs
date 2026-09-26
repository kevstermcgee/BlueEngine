use bluedm::{
    protocol::{WireMessage, PROTOCOL_VERSION},
    server::Server,
};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use vesper3d::viewer::net::{transport::DatagramTransport, UdpTransport};

#[test]
fn two_clients_join_opposite_teams_and_receive_authoritative_snapshot() {
    let mut server = Server::bind("127.0.0.1:0", "smoke-key".into()).unwrap();
    let address = server.local_addr().unwrap();
    let thread = std::thread::spawn(move || {
        server
            .run(Arc::new(AtomicBool::new(false)), Some(30))
            .unwrap()
    });
    let mut clients = [
        UdpTransport::bind("127.0.0.1:0").unwrap(),
        UdpTransport::bind("127.0.0.1:0").unwrap(),
    ];
    let join = WireMessage::Join {
        version: PROTOCOL_VERSION,
        key: "smoke-key".into(),
        name: "smoke".into(),
    }
    .encode()
    .unwrap();
    for client in &clients {
        client.send(address, &join).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut welcomed = [false; 2];
    let mut verified = false;
    while Instant::now() < deadline && !verified {
        for (index, client) in clients.iter_mut().enumerate() {
            for datagram in client.receive().unwrap() {
                match WireMessage::decode(&datagram.data) {
                    Some(WireMessage::Welcome { .. }) => welcomed[index] = true,
                    Some(WireMessage::Snapshot { state, .. }) if state.players.len() == 2 => {
                        assert_ne!(state.players[0].team, state.players[1].team);
                        assert_eq!(state.team_scores, [0, 0]);
                        verified = true;
                    }
                    _ => {}
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    thread.join().unwrap();
    assert_eq!(welcomed, [true, true]);
    assert!(
        verified,
        "two-player authoritative snapshot was not observed"
    );
}
