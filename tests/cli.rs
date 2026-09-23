#![cfg(feature = "offline")]
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "vesper-cli-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn scene(&self, text: &str) {
        fs::write(self.0.join("scene.json"), text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vesper3d"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const SMALL: &str =
    r#"{"size":[32,24],"fps":24,"duration":0.125,"nodes":[{"id":"ball","shape":"sphere"}]}"#;
#[test]
fn contact_supports_extreme_portrait_aspect() {
    let w = Workspace::new();
    w.scene(r#"{"size":[16,3840],"duration":0.1}"#);
    succeeds(w.run(&["contact", "scene.json", "board.png", "--quality", "draft"]));
    let decoder = png::Decoder::new(fs::File::open(w.0.join("board.png")).unwrap());
    let reader = decoder.read_info().unwrap();
    assert_eq!((reader.info().width, reader.info().height), (48, 768));
}
fn succeeds(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn fails(output: Output, contains: &str) {
    assert!(!output.status.success());
    let v: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(v["ok"], false);
    assert!(v["error"].as_str().unwrap().contains(contains), "{v}");
}

#[test]
fn validates_compact_scene() {
    let w = Workspace::new();
    w.scene(SMALL);
    let o = succeeds(w.run(&["validate", "scene.json"]));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["frames"], 3);
}
#[test]
fn png_decodes_and_animation_changes_pixels() {
    let w = Workspace::new();
    w.scene(r#"{"size":[48,32],"duration":2,"nodes":[{"id":"ball","shape":"sphere","pos":[[0,[-2,0,0]],[2,[2,0,0]]]}]}"#);
    succeeds(w.run(&["frame", "scene.json", "a.png", "--quality", "draft"]));
    succeeds(w.run(&[
        "frame",
        "scene.json",
        "b.png",
        "--time",
        "2",
        "--quality",
        "draft",
    ]));
    let decoder = png::Decoder::new(fs::File::open(w.0.join("a.png")).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut data).unwrap();
    assert_eq!((info.width, info.height), (48, 32));
    assert_ne!(
        fs::read(w.0.join("a.png")).unwrap(),
        fs::read(w.0.join("b.png")).unwrap()
    );
}
#[test]
fn rejects_bad_flags_and_nonfinite_time() {
    let w = Workspace::new();
    w.scene(SMALL);
    fails(
        w.run(&["bench", "scene.json", "--quailty", "high"]),
        "unknown option",
    );
    fails(w.run(&["bench", "scene.json", "--time", "NaN"]), "time");
    fails(w.run(&["bench", "scene.json", "--threads", "0"]), "threads");
}
#[test]
fn rejects_malformed_and_unknown_fields() {
    let w = Workspace::new();
    w.scene("{");
    fails(w.run(&["validate", "scene.json"]), "EOF");
    w.scene(r#"{"camra":{}}"#);
    fails(w.run(&["validate", "scene.json"]), "unknown field");
}
#[test]
fn rejects_missing_material_parent_and_odd_dimensions() {
    let w = Workspace::new();
    for (text, needle) in [
        (
            r#"{"nodes":[{"id":"x","material":"typo"}]}"#,
            "unknown material",
        ),
        (
            r#"{"nodes":[{"id":"x","parent":"missing"}]}"#,
            "missing parent",
        ),
        (r#"{"size":[33,32]}"#, "even dimensions"),
    ] {
        w.scene(text);
        fails(w.run(&["validate", "scene.json"]), needle);
    }
}
#[test]
fn rejects_asset_traversal() {
    let w = Workspace::new();
    w.scene(r#"{"nodes":[{"id":"mesh","shape":"mesh","mesh":"../outside.obj"}]}"#);
    fails(w.run(&["validate", "scene.json"]), "relative paths inside");
}
#[test]
fn obj_indices_checked() {
    let w = Workspace::new();
    w.scene(r#"{"nodes":[{"id":"mesh","shape":"mesh","mesh":"broken.obj"}]}"#);
    fs::write(w.0.join("broken.obj"), "v 0 0 0\nf 1 2 3\n").unwrap();
    fails(w.run(&["validate", "scene.json"]), "index out of range");
}
#[test]
fn obj_negative_indices_and_quad() {
    let w = Workspace::new();
    w.scene(r#"{"nodes":[{"id":"mesh","shape":"mesh","mesh":"mesh.obj"}]}"#);
    fs::write(
        w.0.join("mesh.obj"),
        "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4 -3 -2 -1\n",
    )
    .unwrap();
    succeeds(w.run(&["validate", "scene.json"]));
}
#[test]
fn missing_encoder_preserves_existing_output() {
    let w = Workspace::new();
    w.scene(SMALL);
    fs::write(w.0.join("movie.mp4"), b"original").unwrap();
    fails(
        w.run(&[
            "render",
            "scene.json",
            "movie.mp4",
            "--overwrite",
            "--ffmpeg",
            "this-encoder-does-not-exist",
        ]),
        "cannot start FFmpeg",
    );
    assert_eq!(fs::read(w.0.join("movie.mp4")).unwrap(), b"original");
    assert!(!fs::read_dir(&w.0).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("partial")));
}
#[test]
fn existing_output_is_protected() {
    let w = Workspace::new();
    w.scene(SMALL);
    fs::write(w.0.join("still.png"), b"original").unwrap();
    fails(
        w.run(&["frame", "scene.json", "still.png"]),
        "already exists",
    );
    assert_eq!(fs::read(w.0.join("still.png")).unwrap(), b"original");
    fails(
        w.run(&["frame", "scene.json", "scene.json", "--overwrite"]),
        "extension",
    );
}
#[test]
fn actual_mp4_encode_probe_and_decode() {
    if Command::new("ffmpeg").arg("-version").output().is_err()
        || Command::new("ffprobe").arg("-version").output().is_err()
    {
        eprintln!("SKIP: FFmpeg/ffprobe unavailable");
        return;
    }
    let w = Workspace::new();
    w.scene(SMALL);
    succeeds(w.run(&["render", "scene.json", "movie.mp4", "--quality", "draft"]));
    let o = Command::new("ffprobe")
        .current_dir(&w.0)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,width,height,nb_frames,pix_fmt",
            "-of",
            "json",
            "movie.mp4",
        ])
        .output()
        .unwrap();
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["streams"][0]["codec_name"], "h264");
    assert_eq!(v["streams"][0]["width"], 32);
    assert_eq!(v["streams"][0]["nb_frames"], "3");
    assert_eq!(v["streams"][0]["pix_fmt"], "yuv420p");
    assert!(Command::new("ffmpeg")
        .current_dir(&w.0)
        .args(["-v", "error", "-i", "movie.mp4", "-f", "null", "-"])
        .status()
        .unwrap()
        .success());
}
#[test]
fn invalid_audio_cleans_failed_encode() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("SKIP: FFmpeg unavailable");
        return;
    }
    let w = Workspace::new();
    w.scene(r#"{"size":[16,16],"duration":0.1,"audio":"bad.wav"}"#);
    fs::write(w.0.join("bad.wav"), b"not audio").unwrap();
    let out = w.run(&["render", "scene.json", "movie.mp4", "--quality", "draft"]);
    assert!(!out.status.success());
    assert!(!w.0.join("movie.mp4").exists());
    assert!(!fs::read_dir(&w.0).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("partial")));
}
