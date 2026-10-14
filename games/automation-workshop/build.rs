//! Embeds the game's icon and version info into its Windows executable, so Explorer, Task Manager and
//! pinned shortcuts show the game's own art and name instead of a generic application icon.
//!
//! * The icon is `assets/icon.ico` (resource id 1, which Explorer treats as the exe's icon).
//! * `title` and `tagline` come from `assets/identity.json`: `FileDescription` and `ProductName` are the
//!   title, `Comments` is the tagline. `InternalName` / `OriginalFilename` come from its optional `exe`
//!   (default: the package name); the versions come from the package version.
//! * Only the Windows/MSVC window executable is touched (the headless build has no `client` feature).
//! * It uses `rc.exe` from the Windows SDK, found via `RC`, `PATH` or the installed Windows Kits. A missing
//!   SDK or icon is a `cargo:warning=`, never a build failure: the game still gets its window icon at runtime.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON: &str = "assets/icon.ico";
const IDENTITY: &str = "assets/identity.json";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={IDENTITY}");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=RC");
    let target_is = |name: &str, value: &str| env::var(name).is_ok_and(|v| v == value);
    if !target_is("CARGO_CFG_TARGET_OS", "windows")
        || !target_is("CARGO_CFG_TARGET_ENV", "msvc")
        || env::var_os("CARGO_FEATURE_CLIENT").is_none()
    {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let icon = manifest.join(ICON);
    if !icon.is_file() {
        println!("cargo:warning={} is missing: the exe will have no embedded icon", icon.display());
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe (Windows SDK) not found: the exe will have no embedded icon");
        return;
    };
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let (rc_file, res_file) = (out.join("game.rc"), out.join("game.res"));
    // UTF-16 with a byte order mark: rc.exe then reads any title, tagline or path correctly.
    let script = resource_script(&icon, &identity_text(&manifest));
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(script.encode_utf16().flat_map(u16::to_le_bytes));
    if let Err(e) = fs::write(&rc_file, bytes) {
        println!("cargo:warning=could not write {}: {e}", rc_file.display());
        return;
    }
    match Command::new(&rc).args(["/nologo", "/fo"]).arg(&res_file).arg(&rc_file).output() {
        Ok(o) if o.status.success() => println!("cargo:rustc-link-arg-bins={}", res_file.display()),
        Ok(o) => println!(
            "cargo:warning=rc.exe failed ({}): {}{}",
            o.status,
            String::from_utf8_lossy(&o.stdout).trim(),
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => println!("cargo:warning=could not run {}: {e}", rc.display()),
    }
}

/// The text of `assets/identity.json`, or an empty string (with a warning) when it cannot be read.
fn identity_text(manifest: &Path) -> String {
    match fs::read_to_string(manifest.join(IDENTITY)) {
        Ok(text) => text,
        Err(e) => {
            println!("cargo:warning=cannot read {IDENTITY} ({e}): the version info falls back to the package name");
            String::new()
        }
    }
}

/// The resource script: the icon (id 1) and a version block (Task Manager shows FileDescription).
fn resource_script(icon: &Path, identity: &str) -> String {
    let package = |name: &str| env::var(name).unwrap_or_default();
    let number = |name: &str| package(name).parse::<u16>().unwrap_or(0);
    let (major, minor, patch) =
        (number("CARGO_PKG_VERSION_MAJOR"), number("CARGO_PKG_VERSION_MINOR"), number("CARGO_PKG_VERSION_PATCH"));
    let version = match package("CARGO_PKG_VERSION") {
        v if v.is_empty() => format!("{major}.{minor}.{patch}"),
        v => v,
    };
    let title =
        json_string(identity, "title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| package("CARGO_PKG_NAME"));
    let tagline = json_string(identity, "tagline").unwrap_or_else(|| package("CARGO_PKG_DESCRIPTION"));
    let exe =
        json_string(identity, "exe").filter(|e| !e.trim().is_empty()).unwrap_or_else(|| package("CARGO_PKG_NAME"));
    let tagline: String = tagline.chars().take(500).collect();
    // rc.exe reads backslashes in strings as escapes; forward slashes are fine on Windows.
    let icon = rc_escape(&icon.display().to_string().replace('\\', "/"));
    let (title, tagline, exe, version) = (rc_escape(&title), rc_escape(&tagline), rc_escape(&exe), rc_escape(&version));
    format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004L
FILETYPE 0x1L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "{title}"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "{exe}"
            VALUE "OriginalFilename", "{exe}.exe"
            VALUE "ProductName", "{title}"
            VALUE "ProductVersion", "{version}"
            VALUE "Comments", "{tagline}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    )
}

/// Escapes text for an RC string literal: quotes and backslashes are doubled, control characters become spaces.
fn rc_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\"\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

type JsonChars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// The string value of a top-level key of a JSON object, without any dependency. It understands the
/// escapes `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and `\uXXXX` (with surrogate pairs); other values
/// (numbers, arrays, nested objects) are skipped.
fn json_string(text: &str, wanted: &str) -> Option<String> {
    let mut chars = text.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            '"' => {
                let string = json_read_string(&mut chars)?;
                if depth != 1 || string != wanted {
                    continue;
                }
                skip_space(&mut chars);
                if chars.next() != Some(':') {
                    continue; // it was a value that happens to equal the key name
                }
                skip_space(&mut chars);
                return if chars.next() == Some('"') { json_read_string(&mut chars) } else { None };
            }
            _ => {}
        }
    }
    None
}

fn skip_space(chars: &mut JsonChars<'_>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}

/// Reads the rest of a JSON string (the opening quote is already consumed).
fn json_read_string(chars: &mut JsonChars<'_>) -> Option<String> {
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let unit = json_hex4(chars)?;
                    out.push(json_code_point(unit, chars)?);
                }
                other => out.push(other), // covers \" \\ and \/
            },
            c => out.push(c),
        }
    }
}

fn json_hex4(chars: &mut JsonChars<'_>) -> Option<u32> {
    let mut value = 0;
    for _ in 0..4 {
        value = value * 16 + chars.next()?.to_digit(16)?;
    }
    Some(value)
}

/// A UTF-16 unit from `\uXXXX` as a character, joining a following low surrogate escape when needed.
fn json_code_point(unit: u32, chars: &mut JsonChars<'_>) -> Option<char> {
    if (0xD800..0xDC00).contains(&unit) {
        let mut ahead = chars.clone();
        if ahead.next() == Some('\\') && ahead.next() == Some('u') {
            let low = json_hex4(&mut ahead)?;
            if (0xDC00..0xE000).contains(&low) {
                *chars = ahead;
                return char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00));
            }
        }
        return Some('\u{FFFD}');
    }
    Some(char::from_u32(unit).unwrap_or('\u{FFFD}'))
}

fn find_rc() -> Option<PathBuf> {
    if let Some(rc) = env::var_os("RC").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(rc);
    }
    if let Some(found) =
        env::var_os("PATH").and_then(|paths| env::split_paths(&paths).map(|d| d.join("rc.exe")).find(|p| p.is_file()))
    {
        return Some(found);
    }
    // The Windows SDK installs one folder per version: Windows Kits\10\bin\10.0.26100.0\x64\rc.exe.
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };
    ["ProgramFiles(x86)", "ProgramFiles"].iter().filter_map(env::var_os).find_map(|program_files| {
        newest_sdk_rc(&PathBuf::from(program_files).join("Windows Kits").join("10").join("bin"), arch)
    })
}

fn newest_sdk_rc(bin: &Path, arch: &str) -> Option<PathBuf> {
    let mut versions: Vec<(Vec<u32>, PathBuf)> = fs::read_dir(bin)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (name.split('.').filter_map(|part| part.parse().ok()).collect(), p)
        })
        .collect();
    // Compare version folders numerically (10.0.9200.0 is older than 10.0.26100.0); newest first.
    versions.sort();
    versions
        .into_iter()
        .rev()
        .map(|(_, v)| v.join(arch).join("rc.exe"))
        .find(|p| p.is_file())
        .or_else(|| Some(bin.join(arch).join("rc.exe")).filter(|p| p.is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_reads_top_level_keys_only() {
        let text =
            r#"{"list": ["title", {"title": "nested"}], "title": "Real \"Title\" \\ \u00e9 \ud83d\ude00", "n": 3}"#;
        assert_eq!(json_string(text, "title").as_deref(), Some("Real \"Title\" \\ \u{e9} \u{1F600}"));
        assert_eq!(json_string(text, "n"), None);
        assert_eq!(json_string(text, "missing"), None);
        assert_eq!(json_string(r#"{"a": "title", "title": "second"}"#, "title").as_deref(), Some("second"));
    }

    #[test]
    fn json_string_survives_broken_input() {
        assert_eq!(json_string("", "title"), None);
        assert_eq!(json_string(r#"{"title": "unterminated"#, "title"), None);
        assert_eq!(json_string(r#"{"title": "\ud83d alone"}"#, "title").as_deref(), Some("\u{FFFD} alone"));
        assert_eq!(json_string(r#"{"title": "\u12"}"#, "title"), None);
    }

    #[test]
    fn rc_strings_double_quotes_and_backslashes() {
        assert_eq!(rc_escape("say \"hi\" \\ there\n"), "say \"\"hi\"\" \\\\ there ");
    }
}
