//! Looking at and signalling operating-system processes, for the server manager (Unix only).
//!
//! Three things the standard library cannot do: ask whether a pid is alive, send it a signal, and read another process's
//! start time and command line (Linux `/proc`). The two C calls are declared directly (no new dependency); everything else
//! is ordinary file reading.
#![allow(unsafe_code)]

use std::io;

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGKILL: i32 = 9;
pub const SIGTERM: i32 = 15;

/// Send `signal` to `pid`. `Ok(false)` means no such process.
pub fn send_signal(pid: u32, signal: i32) -> io::Result<bool> {
    let pid = i32::try_from(pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "pid out of range"))?;
    // SAFETY: `kill` takes two integers and has no memory preconditions.
    let result = unsafe { kill(pid, signal) };
    if result == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(3) => Ok(false), // ESRCH
        _ => Err(error),
    }
}

/// Is there a live (not zombie) process with this pid?
pub fn is_alive(pid: u32) -> bool {
    match state(pid) {
        Some(state) => state != 'Z' && state != 'X',
        // No /proc (not Linux): fall back to the signal-0 existence check.
        None => !std::path::Path::new("/proc").exists() && send_signal(pid, 0).unwrap_or(false),
    }
}

/// `R`, `S`, `D`, `T`, `Z`, ... from `/proc/PID/stat`, or `None` if the process does not exist or there is no `/proc`.
pub fn state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is in parentheses and may itself contain spaces or parentheses: the state follows the LAST ")".
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().next()?.chars().next()
}

/// The process start time in clock ticks since boot (`/proc/PID/stat` field 22). Together with the pid it identifies one
/// process for good, so a recycled pid is not mistaken for the server that used to have it.
pub fn start_ticks(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = &stat[stat.rfind(')')? + 1..];
    // After ")" the fields are: state(3) ppid(4) ... starttime(22): index 19 counting from state at 0.
    after.split_whitespace().nth(19)?.parse().ok()
}

/// What `/proc` says about another process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub args: Vec<String>,
    pub start_ticks: Option<u64>,
}

/// The command line of a process (NUL-separated in `/proc/PID/cmdline`).
pub fn command_line(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if raw.is_empty() {
        return None; // kernel thread, or already gone
    }
    Some(
        raw.split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect(),
    )
}

/// Every live process whose executable is called `program` (the file name of its first argument), other than this one.
pub fn find_by_program(program: &str) -> Vec<Process> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let me = std::process::id();
    let mut found: Vec<Process> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| *pid != me && is_alive(*pid))
        .filter_map(|pid| {
            let args = command_line(pid)?;
            let exe = std::path::Path::new(args.first()?)
                .file_name()?
                .to_str()?
                .to_owned();
            (exe == program).then(|| Process {
                pid,
                args,
                start_ticks: start_ticks(pid),
            })
        })
        .collect();
    found.sort_by_key(|p| p.pid);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive_and_has_a_start_time_and_a_command_line() {
        let me = std::process::id();
        assert!(is_alive(me));
        assert!(start_ticks(me).is_some());
        assert!(!command_line(me).unwrap().is_empty());
        assert!(matches!(state(me), Some('R' | 'S')));
    }

    #[test]
    fn a_pid_that_does_not_exist_is_not_alive_and_signalling_it_says_so() {
        // The kernel's pid limit is 2^22; this is beyond it.
        let gone = 4_194_999;
        assert!(!is_alive(gone));
        assert!(!send_signal(gone, 0).unwrap());
        assert_eq!(start_ticks(gone), None);
    }

    #[test]
    fn a_child_is_found_by_program_name_and_a_reaped_one_is_not() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id();
        // Give /proc a moment under a loaded machine (this failed once, unreproducibly, in a full parallel run).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut found = find_by_program("sleep");
        while !found.iter().any(|p| p.pid == pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
            found = find_by_program("sleep");
        }
        assert!(
            found
                .iter()
                .any(|p| p.pid == pid && p.args.last().map(String::as_str) == Some("30")),
            "{found:?}"
        );
        assert!(send_signal(pid, SIGTERM).unwrap());
        child.wait().unwrap();
        assert!(!find_by_program("sleep").iter().any(|p| p.pid == pid));
    }

    #[test]
    fn a_zombie_is_not_alive() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        // Give it time to exit without reaping it: it becomes a zombie.
        for _ in 0..100 {
            if state(pid) == Some('Z') {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!is_alive(pid), "state {:?}", state(pid));
        child.wait().unwrap();
    }
}
