//! The toy footrace server: the smallest complete use of [`netplay::cli::serve`], and the game server the hub's
//! tests run. Copy it for your own game: change the game type, the default port, the join-key variable and the
//! participants flag.
//!
//!   be2-toy-server [--listen ADDR] [--transport development|production] [--join-key KEY] [--auto-start SECONDS]
//!                  [--report-dir DIR] [--seed N] [--seats N] [--ai-speed N] [--bots] [--set ID=VALUE]
//!                  [--status-lines] [--exit-on-stdin-eof] [--info] [--help]
use vesper3d::viewer::netplay::cli::{serve, Participants, ServeSpec};
use vesper3d::viewer::netplay::toy::ToyGame;

fn main() -> vesper3d::Result<()> {
    serve::<ToyGame>(&ServeSpec {
        bin_name: "be2-toy-server",
        about: "the toy footrace server (a template for netplay::cli::serve)",
        default_listen: "0.0.0.0:4190",
        default_report_dir: "toy-data",
        join_key_env: "BE2_TOY_JOIN_KEY",
        participants: Participants::Flag {
            flag: "seats",
            min: 1,
            max: 8,
            default: 8,
        },
        default_auto_start: 30,
    })
}
