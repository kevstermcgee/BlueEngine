use crate::{
    geometry::Compiled,
    render::{self, Options},
    scene::asset_path,
    Result,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};
static COUNTER: AtomicU64 = AtomicU64::new(0);
pub struct Pending {
    pub path: PathBuf,
    output: PathBuf,
    overwrite: bool,
}
impl Pending {
    pub fn new(output: &Path, overwrite: bool) -> Result<Self> {
        if output.exists() && !overwrite {
            return Err(format!(
                "output already exists: {}; use --overwrite",
                output.display()
            )
            .into());
        }
        let parent = output
            .parent()
            .filter(|x| !x.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let name = output
            .file_name()
            .ok_or("output needs a filename")?
            .to_string_lossy();
        for _ in 0..100 {
            let path = parent.join(format!(
                ".{name}.partial.{}.{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => {
                    return Ok(Self {
                        path,
                        output: output.to_owned(),
                        overwrite,
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err("could not reserve temporary output".into())
    }
    pub fn commit(self) -> Result<()> {
        if self.overwrite {
            fs::rename(&self.path, &self.output)?;
        } else {
            fs::hard_link(&self.path, &self.output)?;
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
struct Encoder(Child);
impl Drop for Encoder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn png(path: &Path, w: u32, h: u32, data: &[u8], overwrite: bool) -> Result<()> {
    let pending = Pending::new(path, overwrite)?;
    {
        let file = BufWriter::new(File::create(&pending.path)?);
        let mut encoder = png::Encoder::new(file, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(data)?;
        writer.finish()?;
    }
    pending.commit()
}
pub fn video(
    compiled: &Compiled,
    base: &Path,
    path: &Path,
    options: &Options,
    overwrite: bool,
    cancel: &AtomicBool,
    ffmpeg: &str,
) -> Result<serde_json::Value> {
    let s = &compiled.scene;
    let total = s.frames();
    let pending = Pending::new(path, overwrite)?;
    let log = Pending::new(&path.with_extension("encoder-log"), true)?;
    let stderr = File::create(&log.path)?;
    let mut command = Command::new(ffmpeg);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgb24",
        "-video_size",
        &format!("{}x{}", s.size[0], s.size[1]),
        "-framerate",
        &s.fps.to_string(),
        "-i",
        "pipe:0",
    ]);
    if let Some(audio) = &s.audio {
        command
            .args(["-protocol_whitelist", "file,pipe", "-i"])
            .arg(asset_path(base, audio)?);
    }
    command.args([
        "-map",
        "0:v:0",
        "-c:v",
        "libx264",
        "-preset",
        "fast",
        "-crf",
        "18",
        "-pix_fmt",
        "yuv420p",
        "-movflags",
        "+faststart",
        "-threads",
        "2",
    ]);
    if s.audio.is_some() {
        command.args([
            "-map", "1:a:0", "-c:a", "aac", "-b:a", "192k", "-af", "apad",
        ]);
    } else {
        command.arg("-an");
    }
    command
        .args([
            "-t",
            &format!("{:.9}", total as f64 / s.fps as f64),
            "-f",
            "mp4",
        ])
        .arg(&pending.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr));
    let mut encoder = Encoder(
        command
            .spawn()
            .map_err(|e| format!("cannot start FFmpeg ({ffmpeg}): {e}"))?,
    );
    let start = Instant::now();
    let mut stdin = BufWriter::with_capacity(
        s.size[0] as usize * s.size[1] as usize * 3,
        encoder.0.stdin.take().ok_or("FFmpeg stdin unavailable")?,
    );
    let result = (|| -> Result<()> {
        for i in 0..total {
            let data = render::frame(compiled, i as f32 / s.fps as f32, options, cancel)?;
            stdin.write_all(&data)?;
            if i % 30 == 0 || i + 1 == total {
                eprintln!(
                    "{}",
                    serde_json::json!({"frame":i+1,"total":total,"elapsed":start.elapsed().as_secs_f64()})
                );
            }
        }
        stdin.flush()?;
        Ok(())
    })();
    drop(stdin);
    if let Err(e) = result {
        let _ = encoder.0.kill();
        let _ = encoder.0.wait();
        return Err(format!("render failed: {e}; {}", read_log(&log.path)).into());
    }
    let status = encoder.0.wait()?;
    if !status.success() {
        return Err(format!("FFmpeg failed ({status}): {}", read_log(&log.path)).into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("render cancelled before committing output".into());
    }
    pending.commit()?;
    let seconds = start.elapsed().as_secs_f64();
    Ok(
        serde_json::json!({"ok":true,"output":path,"frames":total,"duration":total as f64/s.fps as f64,"render_seconds":seconds,"render_fps":total as f64/seconds,"size":s.size}),
    )
}
fn read_log(path: &Path) -> String {
    let mut s = String::new();
    if let Ok(f) = File::open(path) {
        let _ = f.take(32768).read_to_string(&mut s);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_commit_replaces_only_when_requested() {
        let dir = std::env::temp_dir().join(format!("vesper-commit-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let pending = Pending::new(&path, false).unwrap();
        fs::write(&pending.path, b"first").unwrap();
        pending.commit().unwrap();
        let pending = Pending::new(&path, true).unwrap();
        fs::write(&pending.path, b"second").unwrap();
        pending.commit().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn destination_race_does_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("vesper-race-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        let pending = Pending::new(&path, false).unwrap();
        let temp = pending.path.clone();
        fs::write(&temp, b"our output").unwrap();
        fs::write(&path, b"someone else").unwrap();
        assert!(pending.commit().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"someone else");
        assert!(!temp.exists());
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn preserves_existing_output_and_cleans_temp() {
        let dir = std::env::temp_dir().join(format!("vesper-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("original.txt");
        fs::write(&p, b"original").unwrap();
        assert!(Pending::new(&p, false).is_err());
        let tmp;
        {
            let pending = Pending::new(&p, true).unwrap();
            tmp = pending.path.clone();
            fs::write(&tmp, b"unfinished").unwrap();
        }
        assert!(!tmp.exists());
        assert_eq!(fs::read(&p).unwrap(), b"original");
        fs::remove_file(p).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
