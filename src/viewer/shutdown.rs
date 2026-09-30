//! Stop a server from a signal: the first request saves and exits, a second one exits at once.
//!
//! `be2-headless` used to ignore SIGINT and SIGTERM (its stop flag was never set), so `kill`, `systemctl stop` and
//! `docker stop` ended the process mid-tick with no final autosave. [`install`] makes SIGINT, SIGTERM and SIGHUP on Unix,
//! and Ctrl-C, Ctrl-Break, console close, logoff and system shutdown on Windows, set the stop flag the server loop already
//! watches, so the loop finishes its tick, writes its final save and returns normally (exit status 0).
//!
//! **A second request forces an immediate exit (status 130).** A server that is stuck, or one whose final save is taking
//! too long, can still be stopped by sending the signal again. This is what makes "stop a server that will not stop"
//! a matter of signalling twice rather than `kill -9`.
//!
//! **Windows close and shutdown events** give a process only a few seconds, and the system ends it when the handler
//! returns, so those events wait (up to [`CLOSE_GRACE`]) for the program to call [`finished`] after its final save.
//!
//! No dependency is added (the headless build must not pull in `ctrlc`, see `ARCH-HEADLESS-001`): the two OS calls are
//! declared directly and the handlers only touch atomics (async-signal-safe), with a helper thread carrying the request to
//! the `Arc<AtomicBool>` the loop reads.
#![allow(unsafe_code)]

use std::{
    io,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

/// How many shutdown requests have arrived. The first is graceful; any further one forces an exit.
static REQUESTS: AtomicU32 = AtomicU32::new(0);
/// Set by [`finished`] once the final save is done; Windows close/shutdown events wait for it.
static FINISHED: AtomicBool = AtomicBool::new(false);

/// How long a Windows console-close or system-shutdown event waits for the program to finish saving.
pub const CLOSE_GRACE: Duration = Duration::from_millis(4500);
/// Exit status of a forced exit (the conventional 128 + SIGINT).
pub const FORCED_EXIT_STATUS: i32 = 130;

/// What a shutdown request means for the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// The first request: stop the loop, save, exit normally.
    Graceful,
    /// A request after the first: give up waiting and exit now.
    Force,
}

/// Count one request and say what to do about it. Pure apart from the counter, so it can be tested directly.
pub fn note_request(counter: &AtomicU32) -> Request {
    if counter.fetch_add(1, Ordering::SeqCst) == 0 {
        Request::Graceful
    } else {
        Request::Force
    }
}

/// How many shutdown requests this process has received.
pub fn requests() -> u32 {
    REQUESTS.load(Ordering::SeqCst)
}

/// Tell the signal handlers the program has finished saving and is about to exit. Only Windows close and shutdown events
/// wait for this; calling it on every platform is harmless.
pub fn finished() {
    FINISHED.store(true, Ordering::SeqCst);
}

/// Called from a signal handler: may only touch atomics and async-signal-safe calls.
fn on_signal() -> Request {
    let request = note_request(&REQUESTS);
    if request == Request::Force {
        os::force_exit();
    }
    request
}

/// Make shutdown signals set `stop`. Call once, before the server loop starts; the helper thread ends when `stop` is set.
pub fn install(stop: Arc<AtomicBool>) -> io::Result<()> {
    os::install()?;
    std::thread::Builder::new()
        .name("shutdown-watch".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if requests() > 0 {
                    eprintln!("[Server] Shutdown requested: finishing this tick, saving, then exiting (signal again to exit at once)");
                    stop.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        })?;
    Ok(())
}

#[cfg(unix)]
mod os {
    use super::*;

    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
        fn write(fd: i32, buf: *const u8, count: usize) -> isize;
        fn _exit(status: i32) -> !;
    }
    const SIGHUP: i32 = 1;
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;
    /// `SIG_ERR`, which `signal` returns as an all-ones pointer.
    const SIG_ERR: usize = usize::MAX;

    extern "C" fn handler(_signal: i32) {
        let _ = on_signal();
    }

    pub fn install() -> io::Result<()> {
        for sig in [SIGINT, SIGTERM, SIGHUP] {
            // SAFETY: `handler` is an `extern "C" fn(i32)` that only touches atomics and calls async-signal-safe
            // functions (`write`, `_exit`); `signal` has no other preconditions.
            if unsafe { signal(sig, handler) } == SIG_ERR {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    pub fn force_exit() -> ! {
        const MESSAGE: &[u8] =
            b"[Server] Second shutdown request: exiting immediately without finishing the save\n";
        // SAFETY: `write` and `_exit` are async-signal-safe; the buffer is a static byte string.
        unsafe {
            write(2, MESSAGE.as_ptr(), MESSAGE.len());
            _exit(FORCED_EXIT_STATUS)
        }
    }
}

#[cfg(windows)]
mod os {
    use super::*;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;
    const CTRL_CLOSE_EVENT: u32 = 2;
    const CTRL_LOGOFF_EVENT: u32 = 5;
    const CTRL_SHUTDOWN_EVENT: u32 = 6;

    unsafe extern "system" fn handler(event: u32) -> i32 {
        match event {
            CTRL_C_EVENT | CTRL_BREAK_EVENT => {
                let _ = on_signal();
                1
            }
            CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
                let _ = on_signal();
                // The system ends the process when this returns (or after a few seconds), so wait for the save.
                let deadline = std::time::Instant::now() + CLOSE_GRACE;
                while !FINISHED.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                }
                1
            }
            _ => 0, // not ours: let the next handler run
        }
    }

    pub fn install() -> io::Result<()> {
        // SAFETY: `handler` has the signature `SetConsoleCtrlHandler` expects and outlives the process.
        if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn force_exit() -> ! {
        eprintln!(
            "[Server] Second shutdown request: exiting immediately without finishing the save"
        );
        std::process::exit(FORCED_EXIT_STATUS)
    }
}

#[cfg(not(any(unix, windows)))]
mod os {
    use super::*;
    pub fn install() -> io::Result<()> {
        Ok(())
    }
    pub fn force_exit() -> ! {
        std::process::exit(FORCED_EXIT_STATUS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_request_is_graceful_and_every_later_one_forces_an_exit() {
        let counter = AtomicU32::new(0);
        assert_eq!(note_request(&counter), Request::Graceful);
        assert_eq!(note_request(&counter), Request::Force);
        assert_eq!(note_request(&counter), Request::Force);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
