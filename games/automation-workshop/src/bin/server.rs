//! The existing custom-netplay server lifecycle with the game-owned workshop adapter.
//! Development UDP is the default. Build with `--features production` and select the existing
//! production transport explicitly when encryption is required. This executable cannot reclaim a
//! timed-out seat during its continuous match and does not persist/restart an online workshop.
use automation_workshop::net::WorkshopNet;
use vesper3d::viewer::netplay::cli::{serve, Participants, ServeSpec};

fn main() -> vesper3d::Result<()> {
    serve::<WorkshopNet>(&ServeSpec {
        bin_name: "workshop-server",
        about: "small cooperative automation workshop; full-state native custom netplay",
        default_listen: "0.0.0.0:4192",
        default_report_dir: "workshop-net-reports",
        join_key_env: "WORKSHOP_JOIN_KEY",
        participants: Participants::Fixed(2),
        default_auto_start: 5,
    })
}
