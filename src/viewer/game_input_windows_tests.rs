//! Real Win32 focus and SendInput regression. Software native input is not physical-device evidence.
#![allow(unsafe_code)]
use super::{
    tests::{input, observed_frame},
    *,
};
use crate::viewer::focus::WindowFocus;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant},
};
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    },
    WindowsAndMessaging::{
        AllowSetForegroundWindow, CreateWindowExW, DestroyWindow, DispatchMessageW,
        GetForegroundWindow, PeekMessageW, SetForegroundWindow, ShowWindow, TranslateMessage,
        ASFW_ANY, MSG, PM_REMOVE, SW_RESTORE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    },
};

type Window = windows_sys::Win32::Foundation::HWND;
const NAME: &str =
    "viewer::game_input::windows_tests::native_windows_focus_gates_generated_hotkeys";
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
unsafe fn window(title: &str) -> Window {
    let class = wide("STATIC");
    let title = wide(title);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        30,
        30,
        320,
        200,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null(),
    );
    assert!(
        !hwnd.is_null(),
        "CreateWindowExW requires a native interactive Windows desktop"
    );
    hwnd
}
unsafe fn pump() {
    let mut message: MSG = std::mem::zeroed();
    while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}
unsafe fn settle() {
    // Out-of-context WinEvent callbacks are delivered by this thread's message pump.
    for _ in 0..10 {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
}
unsafe fn foreground(hwnd: Window) {
    ShowWindow(hwnd, SW_RESTORE);
    assert_ne!(
        SetForegroundWindow(hwnd),
        0,
        "Windows refused foreground activation; native check is unverified"
    );
    settle();
    assert_eq!(GetForegroundWindow(), hwnd);
}
unsafe fn key(vk: u16, held: bool) {
    let event = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if held { 0 } else { KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    assert_eq!(
        SendInput(1, &event, std::mem::size_of::<INPUT>() as i32),
        1,
        "SendInput must succeed; this is software-generated native keyboard evidence"
    );
    settle();
    assert_eq!(GetAsyncKeyState(vk as i32) < 0, held);
}
struct Fixture {
    child: Child,
    directory: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // A failed assertion must not leave synthesized held keys in the runner's desktop.
        for vk in [0x74, 0x78, 0x52, 0x0d] {
            let event = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            unsafe {
                SendInput(1, &event, std::mem::size_of::<INPUT>() as i32);
            }
        }
        let _ = std::fs::write(self.directory.join("stop"), b"stop");
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
unsafe fn foreign_fixture(directory: &Path) {
    let hwnd = window("BlueEngine foreign focus fixture");
    AllowSetForegroundWindow(ASFW_ANY);
    std::fs::write(directory.join("window"), format!("{}", hwnd as usize)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !directory.join("stop").exists() && Instant::now() < deadline {
        pump();
        // The parent may switch between the two process-owned windows repeatedly.
        AllowSetForegroundWindow(ASFW_ANY);
        std::thread::sleep(Duration::from_millis(10));
    }
    DestroyWindow(hwnd);
}

fn native_frame(
    input: &mut ClientInput,
    shell: &mut GameShell,
    focus: &mut WindowFocus,
    reader: &dyn Fn(i32) -> i16,
) {
    input.poll_frame(shell, true, Some(reader), |shell, actions| {
        // Match begin_frame_with_key_source: drain devices, observe OS focus once, share acceptance.
        shell.observe_input(true, Some(focus.sample()), true, actions);
    });
}

#[test]
#[ignore = "requires a native Windows desktop; exercised explicitly by Windows CI"]
fn native_windows_focus_gates_generated_hotkeys() {
    unsafe {
        if let Some(directory) = std::env::var_os("BE2_FOREIGN_FOCUS_FIXTURE") {
            foreign_fixture(Path::new(&directory));
            return;
        }
        let own = window("BlueEngine shared adapter native focus test");
        let directory =
            std::env::temp_dir().join(format!("be2-native-focus-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", NAME, "--nocapture"])
            .env("BE2_FOREIGN_FOCUS_FIXTURE", &directory)
            .spawn()
            .unwrap();
        let mut fixture = Fixture { child, directory };
        let deadline = Instant::now() + Duration::from_secs(20);
        let other = loop {
            if let Ok(value) = std::fs::read_to_string(fixture.directory.join("window")) {
                break value.parse::<usize>().unwrap() as Window;
            }
            assert!(
                fixture.child.try_wait().unwrap().is_none(),
                "foreign window fixture exited early"
            );
            assert!(
                Instant::now() < deadline,
                "foreign process did not publish its window"
            );
            pump();
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut focus = WindowFocus::new("BlueEngine shared adapter native focus test");
        let (mut input, mut shell) = (input(), GameShell::in_state(false, false, false));
        let reader = |vk| GetAsyncKeyState(vk);
        for (code, vk) in [
            (KeyCode::F5, 0x74),
            (KeyCode::F9, 0x78),
            (KeyCode::R, 0x52),
            (KeyCode::Enter, 0x0d),
        ] {
            foreground(own);
            native_frame(&mut input, &mut shell, &mut focus, &reader);
            foreground(other);
            key(vk, true);
            native_frame(&mut input, &mut shell, &mut focus, &reader);
            assert!(!input.pressed(code) && !input.down(code));
            assert!(!input.restart_requested(true));
            foreground(own);
            native_frame(&mut input, &mut shell, &mut focus, &reader);
            assert!(
                !input.pressed(code) && !input.restart_requested(true),
                "held return {code:?}"
            );
            key(vk, false);
            native_frame(&mut input, &mut shell, &mut focus, &reader);
            assert!(
                !shell.device_input_allowed(),
                "neutral release frame is suppressed"
            );
            key(vk, true);
            native_frame(&mut input, &mut shell, &mut focus, &reader);
            assert!(
                input.pressed(code),
                "fresh focused {code:?} is accepted, including while paused"
            );
            assert_eq!(
                input.restart_requested(true),
                matches!(code, KeyCode::R | KeyCode::Enter)
            );
            key(vk, false);
            native_frame(&mut input, &mut shell, &mut focus, &reader);
        }
        // A switch out and back before a rendered frame must retain the loss event.
        foreground(other);
        foreground(own);
        let returned = focus.sample();
        assert!(returned.focused && returned.lost);
        observed_frame(&mut input, &mut shell, &reader, returned);
        assert!(!shell.device_input_allowed());
        DestroyWindow(own);
        eprintln!(
            "Win32 foreground events + SendInput F5/F9/R/Enter passed; physical devices untested"
        );
    }
}
