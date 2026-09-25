//! Protocol-independent session management, random credentials, and handshake rate limiting.
use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

/// 128-bit cryptographically secure random session token.
pub type SessionToken = [u64; 2];

/// 128-bit random connection nonce to prevent replay attacks during handshakes.
pub type ConnectionNonce = [u64; 2];

/// Generate an OS-random 128-bit token.
pub fn random_token() -> crate::Result<SessionToken> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("OS randomness unavailable: {e}"))?;
    Ok([
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..].try_into().unwrap()),
    ])
}

/// Token formatting helper for logging.
pub fn format_token(token: SessionToken) -> String {
    format!("{:016x}{:016x}", token[0], token[1])
}

/// Rate limiter for incoming connection handshakes to mitigate denial of service.
pub struct HandshakeLimiter {
    window_start: Instant,
    count: usize,
    max_per_second: usize,
}

impl HandshakeLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
            max_per_second,
        }
    }

    /// Check if a new handshake attempt is permitted under the current rate limit.
    pub fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.count = 0;
        }
        if self.count < self.max_per_second {
            self.count += 1;
            true
        } else {
            false
        }
    }
}

impl Default for HandshakeLimiter {
    fn default() -> Self {
        Self::new(16)
    }
}

/// Information tracking an active authenticated client session.
#[derive(Clone, Debug)]
pub struct SessionEntry<T> {
    pub peer: SocketAddr,
    pub nonce: ConnectionNonce,
    pub token: SessionToken,
    pub last_seen: Instant,
    pub last_input_sequence: u64,
    pub last_command_sequence: u64,
    pub data: T,
}

/// Server session registry supporting authentication, binding, sequence checking, and timeouts.
pub struct SessionRegistry<T> {
    sessions: HashMap<SessionToken, SessionEntry<T>>,
    peer_index: HashMap<SocketAddr, SessionToken>,
    max_sessions: usize,
    timeout: Duration,
}

impl<T> SessionRegistry<T> {
    pub fn new(max_sessions: usize, timeout: Duration) -> Self {
        Self {
            sessions: HashMap::new(),
            peer_index: HashMap::new(),
            max_sessions,
            timeout,
        }
    }

    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_full(&self) -> bool {
        self.sessions.len() >= self.max_sessions
    }

    pub fn get(&self, token: &SessionToken) -> Option<&SessionEntry<T>> {
        self.sessions.get(token)
    }

    pub fn get_mut(&mut self, token: &SessionToken) -> Option<&mut SessionEntry<T>> {
        self.sessions.get_mut(token)
    }

    pub fn get_by_peer(&self, peer: &SocketAddr) -> Option<&SessionEntry<T>> {
        let token = self.peer_index.get(peer)?;
        self.sessions.get(token)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SessionEntry<T>> {
        self.sessions.values()
    }

    pub fn get_by_peer_mut(&mut self, peer: &SocketAddr) -> Option<&mut SessionEntry<T>> {
        let token = *self.peer_index.get(peer)?;
        self.sessions.get_mut(&token)
    }

    /// Register or re-bind a session. Returns the allocated session token if successful.
    pub fn register(
        &mut self,
        peer: SocketAddr,
        nonce: ConnectionNonce,
        now: Instant,
        data: T,
    ) -> crate::Result<SessionToken> {
        // If this peer already has an active session with matching nonce, return existing token
        if let Some(existing_token) = self.peer_index.get(&peer) {
            if let Some(entry) = self.sessions.get_mut(existing_token) {
                if entry.nonce == nonce {
                    entry.last_seen = now;
                    return Ok(*existing_token);
                }
            }
        }

        if self.is_full() {
            return Err("Server session capacity reached".into());
        }

        let token = random_token()?;
        let entry = SessionEntry {
            peer,
            nonce,
            token,
            last_seen: now,
            last_input_sequence: 0,
            last_command_sequence: 0,
            data,
        };

        self.sessions.insert(token, entry);
        self.peer_index.insert(peer, token);
        Ok(token)
    }

    /// Mark an existing session as active at `now`.
    pub fn touch(&mut self, token: &SessionToken, now: Instant) {
        if let Some(s) = self.sessions.get_mut(token) {
            s.last_seen = now;
        }
    }

    /// Validate and advance input sequence number for this session (rejects duplicates and out-of-order stale inputs).
    pub fn accept_input_seq(&mut self, token: &SessionToken, seq: u64, now: Instant) -> bool {
        if let Some(s) = self.sessions.get_mut(token) {
            if seq > s.last_input_sequence {
                s.last_input_sequence = seq;
                s.last_seen = now;
                return true;
            }
        }
        false
    }

    /// Validate and advance reliable command sequence number for this session.
    pub fn accept_command_seq(&mut self, token: &SessionToken, seq: u64, now: Instant) -> bool {
        if let Some(s) = self.sessions.get_mut(token) {
            if seq > s.last_command_sequence {
                s.last_command_sequence = seq;
                s.last_seen = now;
                return true;
            }
        }
        false
    }

    /// Remove a session by token.
    pub fn remove(&mut self, token: &SessionToken) -> Option<SessionEntry<T>> {
        if let Some(entry) = self.sessions.remove(token) {
            self.peer_index.remove(&entry.peer);
            Some(entry)
        } else {
            None
        }
    }

    /// Evict all sessions whose inactivity exceeds `timeout`. Returns list of evicted sessions.
    pub fn evict_timeouts(&mut self, now: Instant) -> Vec<SessionEntry<T>> {
        let timeout = self.timeout;
        let mut expired = Vec::new();
        for (token, entry) in &self.sessions {
            if now.duration_since(entry.last_seen) > timeout {
                expired.push(*token);
            }
        }
        expired
            .into_iter()
            .filter_map(|tok| self.remove(&tok))
            .collect()
    }
}
