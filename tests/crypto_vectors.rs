//! NIST SHA-256 examples and RFC 4231 section 4 HMAC-SHA256 vectors.
//! https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/SHA256.pdf
//! https://www.rfc-editor.org/rfc/rfc4231#section-4
use vesper3d::{runtime::hash::sha256, viewer::net::session::hmac_sha256};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn nist_sha256_vectors() {
    for (message, expected) in [
        (
            &b""[..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &b"abc"[..],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            &b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"[..],
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ] {
        assert_eq!(hex(&sha256(message)), expected);
    }
    assert_eq!(
        hex(&sha256(&vec![b'a'; 1_000_000])),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn rfc4231_hmac_sha256_vectors() {
    let cases = [
        (vec![0x0b; 20], b"Hi There".to_vec(), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"),
        (b"Jefe".to_vec(), b"what do ya want for nothing?".to_vec(), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"),
        (vec![0xaa; 20], vec![0xdd; 50], "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"),
        ((1..=25).collect(), vec![0xcd; 50], "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"),
        (vec![0x0c; 20], b"Test With Truncation".to_vec(), "a3b6167473100ee06e0c796c2955552b"),
        (vec![0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First".to_vec(), "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"),
        (vec![0xaa; 131], b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.".to_vec(), "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"),
    ];
    for (key, message, expected) in cases {
        let tag = hex(&hmac_sha256(&key, &message));
        assert_eq!(&tag[..expected.len()], expected);
    }
}
