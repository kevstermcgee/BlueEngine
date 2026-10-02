//! One hub, one name, every BlueEngine online game (ADR 0037).
//!
//! The always-on front door of a box that hosts games. Players click Play Online, the game asks the hub for
//! "the rooms of *my* game", and they pick one by name or make one; nobody types an IP or a code. The hub is a
//! small supervisor: it answers a tiny datagram protocol on one well-known UDP port, starts one server process per
//! room on a shared pool of ports, closes rooms nobody uses, and restarts each game's permanent Public room if it
//! dies. Games are told apart by a game id on the wire; which executable runs a game, and which settings a room
//! may have, come only from the registry file the box's owner writes ([`registry`]).
//!
//! * [`wire`]: the protocol, `BEHB` v1 (list, create with a source-address cookie, ping).
//! * [`legacy`]: the `DFHB` v1 protocol of the already-shipped Deadfall clients, answered byte for byte.
//! * [`limits`]: the abuse limits (token buckets per source, for creating, and global).
//! * [`registry`]: the config file, `--info` of each game's server, and the checks on both.
//! * [`spawn`] and [`rooms`]: room processes, the port pool and the room lifetimes.
//! * [`mod@serve`]: the datagram handler ([`Hub`]) and the UDP loop. `src/bin/be2-hub.rs` is the executable around it.
//! * [`client`]: what a game links: the non-blocking [`HubClient`], the window-less [`Online`] state machine
//!   behind a Play Online screen, build ids and the default hub address.
//!
//! The servers the hub starts are built with [`cli::serve`](super::cli::serve): `--info`, `--status-lines`,
//! `--exit-on-stdin-eof` and `--set ID=VALUE` are what make a game's server supervisable.
//!
//! Std only, no window: everything here is always compiled and runs headless.
pub mod client;
pub mod legacy;
pub mod limits;
pub mod registry;
pub mod rooms;
pub mod serve;
pub mod spawn;
pub mod wire;

pub use client::{
    build_matches, default_hub, local_build, room_addr, Action, HubClient, HubEvent, Online,
};
pub use limits::Limits;
pub use registry::{
    parse_config, Config, GameEntry, HubSection, HubSettings, ProcessInfo, Registry,
};
pub use rooms::{ManagerConfig, RoomManager};
pub use serve::{serve, Hub, HubOptions};
pub use spawn::ProcessSpawner;
pub use wire::{ErrorCode, Reply, Request, RoomInfo, RoomState};

use std::time::{SystemTime, UNIX_EPOCH};

/// Where the public hub lives: the one place this is written down. `deploy/hub/README.md` explains how the name
/// is kept pointing at the box. A `server.txt` beside the game overrides it without a rebuild.
pub const DEFAULT_HUB: &str = "blue-engine.duckdns.org:4100";

/// `YYYY-MM-DD HH:MM:SSZ` (UTC) for a unix time.
pub fn timestamp(unix_secs: u64) -> String {
    let (days, rem) = (unix_secs / 86_400, unix_secs % 86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// One timestamped line on stdout.
pub fn log(msg: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    println!("[{}] {msg}", timestamp(now));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(timestamp(951_782_400 + 3661), "2000-02-29 01:01:01Z");
        assert_eq!(timestamp(1_790_000_000), "2026-09-21 14:13:20Z");
    }

    #[test]
    fn the_default_hub_is_a_resolvable_address_with_the_wire_default_port() {
        let addr = crate::viewer::devkit::resolve_ipv4("127.0.0.1", wire::DEFAULT_PORT).unwrap();
        assert_eq!(addr.port(), 4100);
        assert!(DEFAULT_HUB.ends_with(":4100") && DEFAULT_HUB.contains("blue-engine"));
    }
}
