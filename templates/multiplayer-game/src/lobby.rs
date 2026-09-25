//! Wire protocol and serialization for game lobby and match replication.
use super::game::{GameAction, GameInput, MatchSnapshot};
use serde::{Deserialize, Serialize};
use vesper3d::viewer::net::session::SessionToken;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PACKET_BYTES: usize = 1100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WireMessage {
    Hello {
        version: u32,
        nonce: SessionToken,
        key: String,
    },
    Welcome {
        nonce: SessionToken,
        token: SessionToken,
        slot: usize,
    },
    Reject {
        nonce: SessionToken,
        reason: String,
    },
    Input {
        token: SessionToken,
        input: GameInput,
    },
    Command {
        token: SessionToken,
        sequence: u64,
        action: GameAction,
    },
    Snapshot {
        token: SessionToken,
        command_ack: u64,
        state: MatchSnapshot,
    },
}

impl WireMessage {
    pub fn encode(&self) -> vesper3d::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_PACKET_BYTES {
            return Err("Packet exceeds 1100 byte MTU budget".into());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_PACKET_BYTES {
            return None;
        }
        serde_json::from_slice(bytes).ok()
    }
}
