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

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
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
        self.register_with_token_source(peer, nonce, now, data, random_token)
    }

    fn register_with_token_source<F>(
        &mut self,
        peer: SocketAddr,
        nonce: ConnectionNonce,
        now: Instant,
        data: T,
        mut token_source: F,
    ) -> crate::Result<SessionToken>
    where
        F: FnMut() -> crate::Result<SessionToken>,
    {
        // If this peer already has an active session with matching nonce, return existing token
        let existing_token = self.peer_index.get(&peer).copied();
        if let Some(existing_token) = existing_token {
            if let Some(entry) = self.sessions.get_mut(&existing_token) {
                if entry.nonce == nonce {
                    entry.last_seen = now;
                    return Ok(existing_token);
                }
            }
        }

        // Rebinding a known peer replaces its old entry and therefore does not consume
        // another capacity slot. Generate the replacement credential before mutating
        // either index so an entropy failure leaves the registry unchanged.
        if existing_token.is_none() && self.is_full() {
            return Err("Server session capacity reached".into());
        }

        let token = token_source()?;
        if self.sessions.contains_key(&token) {
            return Err("OS randomness produced a duplicate session token".into());
        }
        let entry = SessionEntry {
            peer,
            nonce,
            token,
            last_seen: now,
            last_input_sequence: 0,
            last_command_sequence: 0,
            data,
        };

        if let Some(existing_token) = existing_token {
            self.sessions.remove(&existing_token);
        }
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

/// Generate random 16-byte cryptographic salt.
pub fn random_salt() -> crate::Result<[u8; 16]> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| format!("OS randomness unavailable: {e}"))?;
    Ok(salt)
}

/// Generate random connection nonce.
pub fn random_nonce() -> crate::Result<ConnectionNonce> {
    random_token()
}

/// Generate all random material for one authentication challenge.
///
/// No challenge is returned unless both the nonce and salt were obtained from
/// the operating system random source.
pub fn random_auth_challenge() -> crate::Result<(ConnectionNonce, [u8; 16])> {
    auth_challenge_with_sources(random_nonce, random_salt)
}

fn auth_challenge_with_sources<N, S>(
    mut nonce_source: N,
    mut salt_source: S,
) -> crate::Result<(ConnectionNonce, [u8; 16])>
where
    N: FnMut() -> crate::Result<ConnectionNonce>,
    S: FnMut() -> crate::Result<[u8; 16]>,
{
    Ok((nonce_source()?, salt_source()?))
}

/// Constant-time slice comparison to prevent timing side channels.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Compute SHA-256 digest of arbitrary bytes without external dependencies.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while !(msg.len() + 8).is_multiple_of(64) {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, w_item) in w.iter_mut().take(16).enumerate() {
            *w_item = u32::from_be_bytes(chunk[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h_val) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    let mut out = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}

/// Compute standard HMAC-SHA256 for message using secret key.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        let digest = sha256(key);
        key_block[..32].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = [0x5cu8; 64];
    let mut i_key_pad = [0x36u8; 64];
    for i in 0..64 {
        o_key_pad[i] ^= key_block[i];
        i_key_pad[i] ^= key_block[i];
    }

    let mut inner = Vec::with_capacity(64 + msg.len());
    inner.extend_from_slice(&i_key_pad);
    inner.extend_from_slice(msg);
    let inner_hash = sha256(&inner);

    let mut outer = Vec::with_capacity(64 + 32);
    outer.extend_from_slice(&o_key_pad);
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

/// Compute cryptographic challenge response proof for authentication handshake.
pub fn compute_auth_proof(
    key: &str,
    nonce: ConnectionNonce,
    player_id: u64,
    salt: &[u8; 16],
) -> [u8; 32] {
    let mut msg = Vec::with_capacity(40);
    msg.extend_from_slice(&nonce[0].to_le_bytes());
    msg.extend_from_slice(&nonce[1].to_le_bytes());
    msg.extend_from_slice(&player_id.to_le_bytes());
    msg.extend_from_slice(salt);
    hmac_sha256(key.as_bytes(), &msg)
}

/// Verify cryptographic authentication proof in constant time.
pub fn verify_auth_proof(
    key: &str,
    nonce: ConnectionNonce,
    player_id: u64,
    salt: &[u8; 16],
    candidate: &[u8; 32],
) -> bool {
    let expected = compute_auth_proof(key, nonce, player_id, salt);
    constant_time_eq(&expected, candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn registry_rebind_and_cleanup_keep_both_indexes_consistent() {
        let now = Instant::now();
        let mut registry = SessionRegistry::new(1, Duration::from_millis(10));
        let first = registry
            .register_with_token_source(peer(4000), [1, 0], now, 7, || Ok([10, 11]))
            .unwrap();
        assert_eq!(registry.get_by_peer(&peer(4000)).unwrap().token, first);

        let rebound = registry
            .register_with_token_source(peer(4000), [2, 0], now, 7, || Ok([20, 21]))
            .unwrap();
        assert_ne!(first, rebound);
        assert_eq!(registry.count(), 1);
        assert!(registry.get(&first).is_none());
        assert_eq!(registry.get_by_peer(&peer(4000)).unwrap().token, rebound);

        assert_eq!(registry.remove(&rebound).unwrap().data, 7);
        assert_eq!(registry.count(), 0);
        assert!(registry.get_by_peer(&peer(4000)).is_none());
    }

    #[test]
    fn registry_randomness_failure_is_atomic_and_fails_closed() {
        let now = Instant::now();
        let mut registry = SessionRegistry::new(1, Duration::from_secs(5));
        let token = registry
            .register_with_token_source(peer(4001), [1, 0], now, 1, || Ok([30, 31]))
            .unwrap();

        let error = registry
            .register_with_token_source(peer(4001), [2, 0], now, 2, || {
                Err("simulated entropy failure".into())
            })
            .unwrap_err();
        assert!(error.to_string().contains("entropy failure"));
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.get(&token).unwrap().data, 1);
        assert_eq!(registry.get_by_peer(&peer(4001)).unwrap().token, token);
    }

    #[test]
    fn registry_timeout_eviction_clears_peer_index_for_reuse() {
        let now = Instant::now();
        let mut registry = SessionRegistry::new(1, Duration::from_millis(10));
        registry
            .register_with_token_source(peer(4002), [1, 0], now, 3, || Ok([40, 41]))
            .unwrap();

        let expired = registry.evict_timeouts(now + Duration::from_millis(11));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].data, 3);
        assert_eq!(registry.count(), 0);
        assert!(registry.get_by_peer(&peer(4002)).is_none());

        registry
            .register_with_token_source(peer(4003), [1, 0], now, 4, || Ok([50, 51]))
            .unwrap();
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn repeated_join_disconnect_cycles_do_not_exhaust_registry_capacity() {
        let now = Instant::now();
        let mut registry = SessionRegistry::new(2, Duration::from_secs(5));
        for sequence in 0..10u64 {
            let token = registry
                .register_with_token_source(peer(4100), [sequence, 0], now, sequence, || {
                    Ok([sequence + 100, sequence + 200])
                })
                .unwrap();
            assert_eq!(registry.count(), 1);
            assert_eq!(registry.remove(&token).unwrap().data, sequence);
            assert_eq!(registry.count(), 0);
            assert!(registry.get_by_peer(&peer(4100)).is_none());
        }
    }

    #[test]
    fn authentication_challenge_fails_when_either_random_source_fails() {
        let nonce_failure =
            auth_challenge_with_sources(|| Err("nonce entropy unavailable".into()), || Ok([7; 16]))
                .unwrap_err();
        assert!(nonce_failure.to_string().contains("nonce entropy"));

        let salt_failure =
            auth_challenge_with_sources(|| Ok([1, 2]), || Err("salt entropy unavailable".into()))
                .unwrap_err();
        assert!(salt_failure.to_string().contains("salt entropy"));
    }

    #[test]
    fn test_sha256_known_vector() {
        let hash = sha256(b"hello world");
        assert_eq!(
            format!("{:02x?}", hash)
                .chars()
                .filter(|c| c.is_ascii_hexdigit())
                .collect::<String>(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_hmac_sha256_and_auth_proof() {
        let key = "secret_blue_key";
        let nonce = [0x0123456789abcdef, 0xfedcba9876543210];
        let salt = [42u8; 16];
        let player_id = 7;

        let proof = compute_auth_proof(key, nonce, player_id, &salt);
        assert!(verify_auth_proof(key, nonce, player_id, &salt, &proof));

        // Wrong key fails
        assert!(!verify_auth_proof(
            "wrong_key",
            nonce,
            player_id,
            &salt,
            &proof
        ));
        // Wrong player id fails
        assert!(!verify_auth_proof(key, nonce, 8, &salt, &proof));
        // Wrong nonce fails
        assert!(!verify_auth_proof(key, [0, 0], player_id, &salt, &proof));
        // Wrong salt fails
        assert!(!verify_auth_proof(key, nonce, player_id, &[0; 16], &proof));
    }
}
