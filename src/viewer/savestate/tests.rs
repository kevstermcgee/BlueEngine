//! The save container's guarantees, proven by damaging files every way we can think of.
use super::*;
use crate::viewer::devkit::Rng;
use serde_json::json;

fn header() -> SaveHeader {
    SaveHeader {
        saved_at_ms: 1_700_000_000_000,
        ..SaveHeader::new("demo", 3, "Quick save")
            .with_content(0xdead_beef)
            .with_tick(42)
            .with_game("Demo")
    }
}

const PAYLOAD: &[u8] = br#"{"score":7,"name":"orb","big":[1,2,3,4,5,6,7,8,9]}"#;

fn frame() -> Vec<u8> {
    encode(&header(), PAYLOAD).unwrap()
}

/// A directory removed on drop.
struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "savestate-{tag}-{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A frame with arbitrary header bytes and a *valid* checksum, to test header validation on its own.
fn frame_with_header(head: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FRAME_VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(head);
    out.extend_from_slice(payload);
    let digest = sha256(&out);
    out.extend_from_slice(&digest);
    out
}

#[test]
fn a_frame_round_trips_its_header_and_payload() {
    let bytes = frame();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.header, header());
    assert_eq!(decoded.payload, PAYLOAD);
    for payload in [&b""[..], b"{}", &vec![b' '; 1_000_000]] {
        let framed = encode(&header(), payload).unwrap();
        assert_eq!(decode(&framed).unwrap().payload, payload);
    }
}

#[test]
fn every_single_bit_flip_is_detected() {
    let bytes = frame();
    for bit in 0..bytes.len() * 8 {
        let mut damaged = bytes.clone();
        damaged[bit / 8] ^= 1 << (bit % 8);
        assert!(
            decode(&damaged).is_err(),
            "flipping bit {bit} was not detected"
        );
    }
}

#[test]
fn every_truncation_is_detected_and_none_panics() {
    let bytes = frame();
    for len in 0..bytes.len() {
        assert!(
            decode(&bytes[..len]).is_err(),
            "a {len}-byte prefix decoded"
        );
    }
    let mut extended = bytes.clone();
    extended.push(0);
    assert_eq!(decode(&extended).unwrap_err(), SaveError::TrailingBytes);
}

#[test]
fn lengths_that_lie_are_refused_before_anything_is_allocated() {
    let mut bytes = frame();
    bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        decode(&bytes),
        Err(SaveError::TooLarge { what: "header", .. })
    ));
    let mut bytes = frame();
    bytes[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        decode(&bytes),
        Err(SaveError::TooLarge {
            what: "payload",
            ..
        })
    ));
    let mut bytes = frame();
    bytes[16..24].copy_from_slice(&((MAX_PAYLOAD_BYTES as u64) + 1).to_le_bytes());
    assert!(matches!(decode(&bytes), Err(SaveError::TooLarge { .. })));
    let mut bytes = frame();
    bytes[16..24].copy_from_slice(&(MAX_PAYLOAD_BYTES as u64).to_le_bytes());
    assert!(
        matches!(decode(&bytes), Err(SaveError::Truncated { .. })),
        "a plausible length the file cannot back"
    );
    assert!(encode(&header(), &vec![0; MAX_PAYLOAD_BYTES + 1]).is_err());
}

#[test]
fn magic_format_version_and_flags_are_checked() {
    let mut bytes = frame();
    bytes[0] = b'X';
    assert_eq!(decode(&bytes).unwrap_err(), SaveError::NotASave);
    assert_eq!(decode(b"").unwrap_err(), SaveError::NotASave);
    assert_eq!(decode(b"{\"json\":true}").unwrap_err(), SaveError::NotASave);
    let mut bytes = frame();
    bytes[8..10].copy_from_slice(&(FRAME_VERSION + 1).to_le_bytes());
    assert_eq!(
        decode(&bytes).unwrap_err(),
        SaveError::UnsupportedFormat {
            found: FRAME_VERSION + 1,
            supported: FRAME_VERSION
        }
    );
    let mut bytes = frame();
    bytes[8..10].copy_from_slice(&0u16.to_le_bytes());
    assert!(matches!(decode(&bytes), Err(SaveError::Corrupt(_))));
    let mut bytes = frame();
    bytes[10] = 1;
    assert!(matches!(decode(&bytes), Err(SaveError::Corrupt(_))));
    assert!(SaveError::UnsupportedFormat {
        found: 9,
        supported: 1
    }
    .to_string()
    .contains("update the game"));
}

#[test]
fn malformed_headers_are_refused_even_with_a_valid_checksum() {
    let good = serde_json::to_vec(&header()).unwrap();
    assert!(decode(&frame_with_header(&good, b"{}")).is_ok());
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("not json", b"{ nope".to_vec()),
        ("invalid utf-8", vec![b'{', 0xff, 0xfe, b'}']),
        ("empty kind", br#"{"kind":"","version":1}"#.to_vec()),
        (
            "kind with a slash",
            br#"{"kind":"a/b","version":1}"#.to_vec(),
        ),
        (
            "kind too long",
            format!(r#"{{"kind":"{}","version":1}}"#, "k".repeat(65)).into_bytes(),
        ),
        (
            "content not hex",
            br#"{"kind":"demo","content":"zzzz"}"#.to_vec(),
        ),
        (
            "content too long",
            br#"{"kind":"demo","content":"00000000000000000"}"#.to_vec(),
        ),
        (
            "label too long",
            format!(
                r#"{{"kind":"demo","label":"{}"}}"#,
                "x".repeat(MAX_LABEL + 1)
            )
            .into_bytes(),
        ),
        ("array header", b"[1,2,3]".to_vec()),
    ];
    for (name, head) in cases {
        let error = decode(&frame_with_header(&head, b"{}")).expect_err(name);
        assert!(matches!(error, SaveError::Corrupt(_)), "{name}: {error}");
    }
    let bad = SaveHeader {
        kind: String::new(),
        ..header()
    };
    assert!(
        encode(&bad, b"{}").is_err(),
        "the writer refuses what the reader would refuse"
    );
}

#[test]
fn random_damage_never_panics_and_never_loads_a_different_file() {
    let original = frame();
    let mut rng = Rng::new(0x5EED);
    for round in 0..20_000 {
        let mut bytes = original.clone();
        for _ in 0..1 + rng.below(4) {
            match rng.below(6) {
                0 if !bytes.is_empty() => {
                    let i = rng.below(bytes.len());
                    bytes[i] ^= 1 << rng.below(8);
                }
                1 if !bytes.is_empty() => {
                    let i = rng.below(bytes.len());
                    bytes[i] = (rng.next_u64() & 0xff) as u8;
                }
                2 if !bytes.is_empty() => bytes.truncate(rng.below(bytes.len() + 1)),
                3 => bytes.insert(rng.below(bytes.len() + 1), (rng.next_u64() & 0xff) as u8),
                4 if bytes.len() > 4 => {
                    let (a, b) = (rng.below(bytes.len()), rng.below(bytes.len()));
                    bytes.swap(a, b);
                }
                _ => bytes.extend_from_slice(&original[..rng.below(original.len())]),
            }
        }
        if let Ok(decoded) = decode(&bytes) {
            assert_eq!(bytes, original, "round {round}: a damaged file loaded");
            assert_eq!(decoded.payload, PAYLOAD);
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Score {
    score: u32,
    name: String,
    big: Vec<u32>,
}

fn expect(content: Option<u64>) -> Expect<'static> {
    Expect {
        kind: "demo",
        version: 3,
        migrations: &[],
        content,
    }
}

#[test]
fn kind_version_and_content_are_checked_before_the_payload_is_trusted() {
    let bytes = frame();
    let (h, score): (SaveHeader, Score) = load_bytes(&bytes, expect(Some(0xdead_beef))).unwrap();
    assert_eq!((score.score, score.name.as_str(), h.tick), (7, "orb", 42));
    assert!(
        load_bytes::<Score>(&bytes, expect(None)).is_ok(),
        "None accepts any content"
    );
    let other = Expect {
        kind: "other",
        ..expect(None)
    };
    assert!(matches!(
        load_bytes::<Score>(&bytes, other),
        Err(SaveError::WrongKind { .. })
    ));
    assert!(matches!(
        load_bytes::<Score>(&bytes, expect(Some(1))),
        Err(SaveError::WrongContent { .. })
    ));
    let older_build = Expect {
        version: 2,
        ..expect(None)
    };
    assert_eq!(
        load_bytes::<Score>(&bytes, older_build).unwrap_err(),
        SaveError::NewerVersion {
            kind: "demo".into(),
            found: 3,
            supported: 2
        }
    );
    let unbound = encode(
        &SaveHeader {
            content: String::new(),
            ..header()
        },
        PAYLOAD,
    )
    .unwrap();
    assert!(
        matches!(load_bytes::<Score>(&unbound, expect(Some(5))), Err(SaveError::WrongContent { found, .. }) if found == "(none)"),
        "a save with no fingerprint cannot dodge the content check"
    );
    let wrong_shape = encode(&header(), br#"{"score":"seven"}"#).unwrap();
    assert!(matches!(
        load_bytes::<Score>(&wrong_shape, expect(None)),
        Err(SaveError::Invalid(_))
    ));
    let not_json = encode(&header(), b"not json").unwrap();
    assert!(matches!(
        load_bytes::<Score>(&not_json, expect(None)),
        Err(SaveError::Invalid(_))
    ));
    let text = SaveError::WrongContent {
        expected: "a".into(),
        found: "b".into(),
    }
    .to_string();
    assert!(text.contains("different content"));
}

fn rename_score(mut v: Value) -> Result<Value, String> {
    let object = v.as_object_mut().ok_or("not an object")?;
    let old = object.remove("points").ok_or("no points")?;
    object.insert("score".into(), old);
    Ok(v)
}

fn add_name(mut v: Value) -> Result<Value, String> {
    v.as_object_mut()
        .ok_or("not an object")?
        .insert("name".into(), "unnamed".into());
    Ok(v)
}

fn add_big(mut v: Value) -> Result<Value, String> {
    v.as_object_mut()
        .ok_or("not an object")?
        .insert("big".into(), json!([]));
    Ok(v)
}

fn always_fails(_: Value) -> Result<Value, String> {
    Err("cannot be upgraded".into())
}

#[test]
fn old_payloads_are_upgraded_step_by_step_and_a_missing_step_is_an_error() {
    let steps = [
        Migration {
            from: 1,
            step: rename_score,
        },
        Migration {
            from: 2,
            step: add_name,
        },
    ];
    let v1 = encode(
        &SaveHeader {
            version: 1,
            ..header()
        },
        br#"{"points":9}"#,
    )
    .unwrap();
    let v2 = encode(
        &SaveHeader {
            version: 2,
            ..header()
        },
        br#"{"score":9}"#,
    )
    .unwrap();
    let steps3 = [
        steps[0],
        steps[1],
        Migration {
            from: 3,
            step: add_big,
        },
    ];
    let expect4 = Expect {
        version: 4,
        migrations: &steps3,
        ..expect(None)
    };
    let (_, s): (_, Score) = load_bytes(&v1, expect4).unwrap();
    assert_eq!((s.score, s.name.as_str(), s.big.len()), (9, "unnamed", 0));
    let (_, s): (_, Score) = load_bytes(&v2, expect4).unwrap();
    assert_eq!(s.name, "unnamed");
    let gap = Expect {
        version: 4,
        migrations: &[steps[1]],
        ..expect(None)
    };
    assert!(
        matches!(load_bytes::<Score>(&v1, gap), Err(SaveError::Invalid(why)) if why.contains("no migration from save version 1"))
    );
    let failing = Expect {
        version: 2,
        migrations: &[Migration {
            from: 1,
            step: always_fails,
        }],
        ..expect(None)
    };
    assert!(
        matches!(load_bytes::<Score>(&v1, failing), Err(SaveError::Invalid(why)) if why.contains("cannot be upgraded"))
    );
    assert_eq!(
        migrate(json!({"a": 1}), 5, 5, &[]).unwrap(),
        json!({"a": 1}),
        "nothing to do at the current version"
    );
}

#[test]
fn atomic_writes_create_directories_and_read_back() {
    let dir = Dir::new("atomic");
    let path = dir.file("nested/deeper/quick.be2save");
    write_atomic(&path, &frame()).unwrap();
    let loaded = read_save(&path).unwrap();
    assert_eq!((loaded.header, loaded.source), (header(), Source::Primary));
    assert_eq!(loaded.payload, PAYLOAD);
    assert!(
        !backup_path(&path).exists(),
        "the first save has nothing to back up"
    );
    assert_eq!(
        fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1,
        "no temporary file is left behind"
    );
}

#[test]
fn a_second_save_keeps_the_first_as_backup_and_a_corrupt_primary_never_replaces_the_backup() {
    let dir = Dir::new("backup");
    let path = dir.file("quick.be2save");
    let first = encode(
        &SaveHeader {
            label: "first".into(),
            ..header()
        },
        b"{\"n\":1}",
    )
    .unwrap();
    let second = encode(
        &SaveHeader {
            label: "second".into(),
            ..header()
        },
        b"{\"n\":2}",
    )
    .unwrap();
    let third = encode(
        &SaveHeader {
            label: "third".into(),
            ..header()
        },
        b"{\"n\":3}",
    )
    .unwrap();
    write_atomic(&path, &first).unwrap();
    write_atomic(&path, &second).unwrap();
    assert_eq!(fs::read(&path).unwrap(), second);
    assert_eq!(
        fs::read(backup_path(&path)).unwrap(),
        first,
        "the previous save is the backup"
    );
    // Damage the primary; the next save must not rotate the damaged file over the good backup.
    let mut damaged = second.clone();
    damaged[40] ^= 0xff;
    fs::write(&path, &damaged).unwrap();
    write_atomic(&path, &third).unwrap();
    assert_eq!(fs::read(&path).unwrap(), third);
    assert_eq!(
        fs::read(backup_path(&path)).unwrap(),
        first,
        "the last good backup survives a corrupt primary"
    );
}

#[test]
fn a_damaged_or_missing_primary_falls_back_to_the_backup_and_says_so() {
    let dir = Dir::new("fallback");
    let path = dir.file("quick.be2save");
    assert!(matches!(read_save(&path), Err(SaveError::NotFound(_))));
    let one = encode(
        &SaveHeader {
            label: "one".into(),
            ..header()
        },
        b"{}",
    )
    .unwrap();
    let two = encode(
        &SaveHeader {
            label: "two".into(),
            ..header()
        },
        b"{}",
    )
    .unwrap();
    write_atomic(&path, &one).unwrap();
    write_atomic(&path, &two).unwrap();
    fs::write(&path, b"garbage").unwrap();
    let loaded = read_save(&path).unwrap();
    assert_eq!(loaded.header.label, "one");
    assert!(matches!(loaded.source, Source::Backup(SaveError::NotASave)));
    fs::remove_file(&path).unwrap();
    let loaded = read_save(&path).unwrap();
    assert!(matches!(
        loaded.source,
        Source::Backup(SaveError::NotFound(_))
    ));
    fs::write(backup_path(&path), b"also garbage").unwrap();
    assert!(
        matches!(read_save(&path), Err(SaveError::NotFound(_))),
        "both bad: the primary's error is reported"
    );
}

#[test]
fn a_failed_write_leaves_the_old_file_and_no_temporary_file() {
    let dir = Dir::new("failure");
    let path = dir.file("quick.be2save");
    write_atomic(&path, &frame()).unwrap();
    // A frame that does not verify is refused before anything is touched.
    let mut bad = frame();
    bad[30] ^= 1;
    assert!(write_atomic(&path, &bad).is_err());
    assert_eq!(fs::read(&path).unwrap(), frame());
    // The target is a directory: the rename fails, the temp file is cleaned up.
    let blocked = dir.file("blocked.be2save");
    fs::create_dir_all(&blocked).unwrap();
    assert!(write_atomic(&blocked, &frame()).is_err());
    assert!(blocked.is_dir());
    let stray: Vec<_> = fs::read_dir(&dir.0)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(stray.is_empty(), "left over: {stray:?}");
}

#[test]
fn slot_names_are_restricted_to_safe_lowercase_identifiers() {
    for good in ["quick", "auto", "auto-2", "slot_3", "a", &"x".repeat(48)] {
        assert!(valid_slot(good), "{good}");
    }
    let too_long = "x".repeat(49);
    for bad in [
        "", "Quick", "a b", "../x", "a/b", "a\\b", "x.y", "con", "nul", "com1", "lpt9", "é",
        &too_long,
    ] {
        assert!(!valid_slot(bad), "{bad:?}");
    }
    let slots = SaveSlots::new(std::env::temp_dir().join("never-created-slots"));
    assert!(matches!(
        slots.save("../escape", &header(), b"{}"),
        Err(SaveError::BadSlot(_))
    ));
    assert!(matches!(slots.load("Quick"), Err(SaveError::BadSlot(_))));
    assert!(!slots.dir().exists(), "a bad slot name creates nothing");
}

#[test]
fn slots_save_load_list_and_delete() {
    let dir = Dir::new("slots");
    let slots = SaveSlots::new(&dir.0);
    assert!(
        slots.list().unwrap().is_empty(),
        "no directory yet is an empty list, not an error"
    );
    let stamp = |ms, label: &str| SaveHeader {
        saved_at_ms: ms,
        label: label.into(),
        ..header()
    };
    slots
        .save("old", &stamp(1_000, "Old"), b"{\"n\":1}")
        .unwrap();
    slots
        .save("new", &stamp(3_000, "New"), b"{\"n\":2}")
        .unwrap();
    slots
        .save("mid", &stamp(2_000, "Mid"), b"{\"n\":3}")
        .unwrap();
    slots
        .save("mid", &stamp(2_500, "Mid again"), b"{\"n\":4}")
        .unwrap();
    fs::write(dir.file("notes.txt"), "not a slot").unwrap();
    fs::write(dir.file("Bad Name.be2save"), "not a slot either").unwrap();
    let listing = slots.list().unwrap();
    let order: Vec<_> = listing.iter().map(|i| i.slot.as_str()).collect();
    assert_eq!(order, ["new", "mid", "old"], "newest first; strays ignored");
    assert!(listing.iter().all(|i| i.header.is_ok() && i.bytes > 100));
    assert_eq!(listing[1].header.as_ref().unwrap().label, "Mid again");
    assert!(listing[1].has_backup && !listing[0].has_backup);
    assert_eq!(slots.load("mid").unwrap().payload, b"{\"n\":4}");
    assert!(slots.exists("mid") && !slots.exists("nope"));
    slots.delete("mid").unwrap();
    assert!(!slots.exists("mid") && !backup_path(&slots.path("mid").unwrap()).exists());
    slots.delete("mid").unwrap();
    assert!(matches!(slots.load("mid"), Err(SaveError::NotFound(_))));
}

#[test]
fn listing_reports_damaged_slots_instead_of_offering_them() {
    let dir = Dir::new("damaged");
    let slots = SaveSlots::new(&dir.0);
    slots.save("good", &header(), b"{}").unwrap();
    slots.save("bad", &header(), b"{}").unwrap();
    let path = slots.path("bad").unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&path, bytes).unwrap();
    let listing = slots.list().unwrap();
    let bad = listing.iter().find(|i| i.slot == "bad").unwrap();
    assert!(matches!(bad.header, Err(SaveError::Corrupt(_))));
    assert!(listing
        .iter()
        .find(|i| i.slot == "good")
        .unwrap()
        .header
        .is_ok());
    // The cheap header peek says what the file claims without verifying it.
    assert_eq!(
        peek_header(&slots.path("good").unwrap()).unwrap().label,
        "Quick save"
    );
    assert!(peek_header(&dir.file("missing.be2save")).is_err());
    fs::write(
        dir.file("junk.be2save"),
        b"nope nope nope nope nope nope nope",
    )
    .unwrap();
    assert_eq!(
        peek_header(&dir.file("junk.be2save")).unwrap_err(),
        SaveError::NotASave
    );
}

#[test]
fn a_ring_of_autosaves_keeps_the_newest_few_in_order() {
    let dir = Dir::new("ring");
    let slots = SaveSlots::new(&dir.0);
    for n in 1..=5 {
        let head = SaveHeader {
            label: format!("auto {n}"),
            ..header()
        };
        slots
            .save_ring(AUTO_SLOT, 3, &head, format!("{{\"n\":{n}}}").as_bytes())
            .unwrap();
    }
    let label = |slot: &str| slots.load(slot).unwrap().header.label;
    assert_eq!(
        (
            label("auto").as_str(),
            label("auto-2").as_str(),
            label("auto-3").as_str()
        ),
        ("auto 5", "auto 4", "auto 3")
    );
    assert!(!slots.exists("auto-4"), "only three are kept");
    assert!(slots.save_ring("Bad Base", 3, &header(), b"{}").is_err());
}

#[test]
fn errors_read_as_plain_sentences() {
    let all = [
        SaveError::NotASave,
        SaveError::Truncated {
            expected: 100,
            found: 10,
        },
        SaveError::TrailingBytes,
        SaveError::Corrupt("checksum".into()),
        SaveError::TooLarge {
            what: "payload",
            limit: 5,
        },
        SaveError::WrongKind {
            expected: "a".into(),
            found: "b".into(),
        },
        SaveError::Invalid("why".into()),
        SaveError::BadSlot("x y".into()),
        SaveError::NewerVersion {
            kind: "k".into(),
            found: 3,
            supported: 2,
        },
        SaveError::NotFound("x".into()),
        SaveError::Io {
            path: "saves/quick.be2save".into(),
            message: "access denied".into(),
        },
    ];
    for error in all {
        let text = error.to_string();
        assert!(
            text.len() > 8 && !text.contains("Some(") && !text.contains('{'),
            "{text}"
        );
    }
}
