/// Compute the standard SHA-256 digest, preserving the snapshot/content hash format.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    ring::digest::digest(&ring::digest::SHA256, data)
        .as_ref()
        .try_into()
        .expect("SHA-256 digest is 32 bytes")
}
