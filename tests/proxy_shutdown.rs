//! The ordinary headless tooling proxy handles Ctrl-C without the cinematic renderer.
#![cfg(unix)]
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
struct Proxy(Child);
impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn ctrl_c_stops_proxy_cleanly() {
    let mut proxy = Proxy(
        Command::new(env!("CARGO_BIN_EXE_be2-tools"))
            .args(["net-proxy", "127.0.0.1:0", "127.0.0.1:9", "bad-wifi"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = proxy.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        let _ = tx.send(line);
    });
    let ready = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("proxy readiness");
    assert!(ready.contains("proxy_listening"), "{ready}");
    assert_eq!(unsafe { kill(proxy.0.id() as i32, 2) }, 0);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = proxy.0.try_wait().unwrap() {
            assert!(status.success(), "Ctrl-C must return success: {status}");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "proxy did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
    reader.join().unwrap();
}
