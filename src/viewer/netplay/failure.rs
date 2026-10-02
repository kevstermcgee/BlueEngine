//! Why a connection attempt failed, as a type a game can show properly.
//!
//! [`ClientState::Rejected`](super::ClientState) carries a string, and "The server turned you away: Could not
//! reach the server" reads like a refusal when nobody answered at all. [`ConnectFailure`] tells the cases
//! apart, and [`ConnectFailure::hint`] is one short sentence a player can act on. The reason strings the
//! server sends live here as constants, used by both the server (to send) and the client (to classify), so
//! they cannot drift apart.
use std::net::SocketAddr;

/// The server and the client disagree about the game build (fingerprint).
pub const REASON_VERSION: &str = "Game version differs from the server; update the game";
/// The join key was wrong.
pub const REASON_KEY: &str = "Wrong join key";
/// A match is running; the lobby reopens afterwards.
pub const REASON_MATCH: &str = "A match is in progress; try again in a moment";
/// Prefix of the full-server reason; [`full_reason`] appends the seat count.
pub const REASON_FULL_PREFIX: &str = "Server is full";

/// The full-server reason for a game with `max_seats` seats.
pub fn full_reason(max_seats: usize) -> String {
    format!("{REASON_FULL_PREFIX} ({max_seats} players)")
}

/// Why connecting failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectFailure {
    /// Nobody answered: offline, wrong address, or a port that is not forwarded.
    Unreachable { addr: SocketAddr, waited_secs: u32 },
    /// The server runs a different build of the game.
    VersionMismatch,
    /// The server wants a join key and ours was wrong.
    WrongKey,
    /// A match is running; the server takes players between rounds.
    MatchInProgress,
    /// Every seat is taken.
    Full,
    /// Any other server answer, verbatim.
    Other(String),
}

impl ConnectFailure {
    /// Sort a reason string from a server's `Rejected` into a case. Exact constants first, then loose
    /// keywords so a server built from a slightly different engine revision still classifies.
    pub fn classify(reason: &str) -> ConnectFailure {
        if reason == REASON_VERSION {
            return Self::VersionMismatch;
        }
        if reason == REASON_KEY {
            return Self::WrongKey;
        }
        if reason == REASON_MATCH {
            return Self::MatchInProgress;
        }
        if reason.starts_with(REASON_FULL_PREFIX) {
            return Self::Full;
        }
        let lower = reason.to_ascii_lowercase();
        if lower.contains("version") {
            Self::VersionMismatch
        } else if lower.contains("join key") || lower.contains("wrong key") {
            Self::WrongKey
        } else if lower.contains("match") && lower.contains("progress") {
            Self::MatchInProgress
        } else if lower.contains("full") {
            Self::Full
        } else {
            Self::Other(reason.to_string())
        }
    }

    /// One short sentence for the player that says what to do next.
    pub fn hint(&self) -> String {
        match self {
            Self::Unreachable { addr, waited_secs } => format!(
                "No reply from {addr} after {waited_secs} s: the server may be offline, the address may be \
                 wrong, or its port may not be forwarded."
            ),
            Self::VersionMismatch => {
                "The server runs a different version of the game. Update the game (or ask the host to) and try again."
                    .into()
            }
            Self::WrongKey => "The server wants a join key and this one was not accepted. Check it with the host.".into(),
            Self::MatchInProgress => {
                "A match is in progress. The server will accept you between rounds, so try again in a moment."
                    .into()
            }
            Self::Full => "The server is full. Try again when someone leaves.".into(),
            Self::Other(reason) => format!("The server turned you away: {reason}."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_server_reason_classifies_and_unknown_ones_survive() {
        assert_eq!(
            ConnectFailure::classify(REASON_VERSION),
            ConnectFailure::VersionMismatch
        );
        assert_eq!(
            ConnectFailure::classify(REASON_KEY),
            ConnectFailure::WrongKey
        );
        assert_eq!(
            ConnectFailure::classify(REASON_MATCH),
            ConnectFailure::MatchInProgress
        );
        assert_eq!(
            ConnectFailure::classify(&full_reason(8)),
            ConnectFailure::Full
        );
        assert_eq!(
            ConnectFailure::classify("Server is full"),
            ConnectFailure::Full
        );
        assert_eq!(
            ConnectFailure::classify("Banned"),
            ConnectFailure::Other("Banned".into())
        );
    }

    #[test]
    fn hints_are_one_actionable_sentence_and_name_the_address() {
        let addr: SocketAddr = "203.0.113.5:27015".parse().unwrap();
        let hint = ConnectFailure::Unreachable {
            addr,
            waited_secs: 8,
        }
        .hint();
        assert!(
            hint.contains("203.0.113.5:27015") && hint.contains("8 s"),
            "{hint}"
        );
        assert!(
            hint.contains("forwarded") && hint.contains("offline"),
            "{hint}"
        );
        assert!(ConnectFailure::MatchInProgress
            .hint()
            .contains("between rounds"));
        for f in [
            ConnectFailure::VersionMismatch,
            ConnectFailure::WrongKey,
            ConnectFailure::Full,
            ConnectFailure::Other("x".into()),
        ] {
            assert!(!f.hint().is_empty());
        }
    }
}
