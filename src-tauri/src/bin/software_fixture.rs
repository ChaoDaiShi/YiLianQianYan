//! Explicit local-only Win32 acceptance harness; never attaches any user application.
#![cfg_attr(not(debug_assertions), allow(dead_code))]
#[cfg(not(debug_assertions))]
compile_error!("Software fixture must not be built in release mode");
#[cfg(windows)]
#[path = "../software/fixture_input.rs"]
mod input;
#[path = "../software/mod.rs"]
mod software;

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows software fixture unavailable on this platform");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    if let Err(e) = fixture::run() {
        eprintln!("software_fixture_failed: {e}");
        std::process::exit(1);
    }
}
#[cfg(windows)]
mod fixture {
    use super::software::NativeSoftwareHost;
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    use windows::{
        core::w,
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                HiDpi::{
                    SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_CONTEXT_UNAWARE,
                },
                WindowsAndMessaging::*,
            },
        },
    };
    use yilian_backend::software::{Registration, RegistrationInput, SoftwareHost, Viewport};
    static INVOCATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_APP => {
                // Hold only our owned target queue while a cross-thread show/focus is requested.
                std::thread::sleep(Duration::from_millis(200));
                LRESULT(0)
            }
            WM_COMMAND if wparam.0 & 0xffff == 301 => {
                let count = INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                let _ = SetWindowTextW(
                    HWND(lparam.0 as *mut _),
                    if count == 0 {
                        w!("Applied")
                    } else {
                        w!("Applied again")
                    },
                );
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
    unsafe fn create_window(label: &str) -> Result<HWND, String> {
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("YiLianSoftwareOwnedFixture"),
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
        let title = label.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class.lpszClassName,
            windows::core::PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            80,
            80,
            760,
            520,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        if label != "Software fixture host" {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("BUTTON"),
                w!("Apply"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                20,
                20,
                140,
                36,
                Some(hwnd),
                Some(HMENU(301usize as *mut _)),
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER,
                20,
                70,
                300,
                36,
                Some(hwnd),
                Some(HMENU(302usize as *mut _)),
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("BUTTON"),
                w!("Word wrap"),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
                20,
                175,
                300,
                36,
                Some(hwnd),
                Some(HMENU(304usize as *mut _)),
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_PASSWORD as u32),
                20,
                120,
                300,
                36,
                Some(hwnd),
                Some(HMENU(303usize as *mut _)),
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(hwnd)
    }
    unsafe fn pump() {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    fn witness(name: &str) {
        println!("{{\"check\":\"{name}\",\"passed\":true}}");
    }
    fn require(value: bool, detail: &str) -> Result<(), String> {
        if value {
            Ok(())
        } else {
            Err(detail.into())
        }
    }
    pub fn run() -> Result<(), String> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let two_windows = std::env::args().any(|a| a == "--target");
        let launched_target = std::env::var("YILIAN_SOFTWARE_FIXTURE_TARGET").as_deref() == Ok("1");
        if two_windows || launched_target {
            unsafe {
                let first = create_window("Software fixture target A")?;
                let second = if two_windows {
                    let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE);
                    let result = create_window("Software fixture target B");
                    let _ = SetThreadDpiAwarenessContext(previous);
                    Some(result?)
                } else {
                    None
                };
                println!(
                    "READY:{}:{}",
                    first.0 as usize,
                    second.map_or(0, |h| h.0 as usize)
                );
                let _ = std::io::stdout().flush();
                pump();
            }
            return Ok(());
        }
        let (send, receive) = mpsc::channel();
        let thread = std::thread::spawn(move || unsafe {
            let hwnd = create_window("Software fixture host");
            let valid = hwnd.as_ref().ok().map(|h| h.0 as usize);
            let _ = send.send(hwnd.map(|h| h.0 as usize));
            if valid.is_some() {
                pump();
            }
        });
        let parent = receive
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())??;
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--target")
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut ready = String::new();
        BufReader::new(child.stdout.take().ok_or("No fixture stdout")?)
            .read_line(&mut ready)
            .map_err(|e| e.to_string())?;
        let host = std::sync::Arc::new(NativeSoftwareHost::new());
        host.set_parent(parent as u64)?;
        let result = (|| -> Result<(), String> {
            let mut client = windows::Win32::Foundation::RECT::default();
            unsafe {
                GetClientRect(HWND(parent as *mut _), &mut client).map_err(|e| e.to_string())?;
            }
            let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(HWND(parent as *mut _)) }
                .max(96);
            let viewport = Viewport {
                x: 10,
                y: 10,
                width: ((client.right as f64 * 96.0 / dpi as f64).floor() as u32)
                    .saturating_sub(20)
                    .min(600),
                height: ((client.bottom as f64 * 96.0 / dpi as f64).floor() as u32)
                    .saturating_sub(20)
                    .min(350),
            };
            println!(
                "{}",
                serde_json::json!({"check":"native_viewport_metadata","dpi":dpi,"client_width":client.right,"client_height":client.bottom,"logical_viewport":viewport})
            );
            require(
                ready.starts_with("READY:"),
                "Target fixture did not become ready",
            )?;
            let registration = Registration {
                id: "fixture-only".into(),
                revision: 1,
                config: RegistrationInput {
                    name: "Owned fixture".into(),
                    adapter: "generic_fixture".into(),
                    executable: std::env::current_exe()
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .to_string(),
                    workspace_id: None,
                    allow_observe: true,
                    allow_content: false,
                    allow_window_ocr: false,
                    allow_actions: true,
                },
            };
            let own = yilian_backend::software::WindowIdentity {
                hwnd: parent.to_string(),
                pid: std::process::id(),
                process_started_at: 0,
                executable: registration.config.executable.clone(),
                class_name: "YiLianSoftwareOwnedFixture".into(),
            };
            require(
                host.verify(&own)
                    .expect_err("Host window must never be an AI target")
                    .code
                    == "software_host_target_forbidden",
                "Assistant self-target was not rejected",
            )?;
            witness("assistant_self_target_rejected");
            let candidates = host.discover(&registration).map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::json!({"check":"candidate_metadata","expected_pid":child.id(),"candidates":candidates})
            );
            for raw in ready
                .trim()
                .split(':')
                .skip(1)
                .filter_map(|value| value.parse::<usize>().ok())
                .filter(|value| *value != 0)
            {
                println!(
                    "{}",
                    serde_json::json!({"check":"owned_ready_window_metadata","hwnd":raw,"identity":super::software::fixture_candidate(raw)})
                );
            }
            let targets = candidates
                .into_iter()
                .filter(|w| {
                    w.identity.pid == child.id()
                        && w.identity.class_name == "YiLianSoftwareOwnedFixture"
                })
                .collect::<Vec<_>>();
            require(
                targets.len() == 2,
                "Distinct same-process windows were not enumerated",
            )?;
            witness("multi_instance_exact_candidates");
            let target = &targets
                .iter()
                .find(|w| {
                    host.verify(&w.identity)
                        .is_ok_and(|s| s.embedding_supported)
                })
                .ok_or("No matching-DPI fixture candidate")?
                .identity;
            let mismatched = &targets
                .iter()
                .find(|w| {
                    host.verify(&w.identity)
                        .is_ok_and(|s| !s.embedding_supported)
                })
                .ok_or("No mixed-DPI fixture candidate")?
                .identity;
            let fallback = host
                .present(mismatched, "embedded", Some(viewport))
                .map_err(|e| e.to_string())?;
            require(
                fallback.mode == "independent" && !fallback.embedding_supported,
                "Mismatched DPI window was reparented",
            )?;
            witness("dpi_mismatch_independent_fallback");
            let mut wrong = target.clone();
            wrong.process_started_at += 1;
            require(
                host.verify(&wrong).is_err(),
                "Recycled-process identity was accepted",
            )?;
            witness("stale_identity_rejected");
            let before = host.verify(target).map_err(|e| e.to_string())?;
            let native_target =
                HWND(target.hwnd.parse::<usize>().map_err(|e| e.to_string())? as *mut _);
            unsafe {
                let _ = ShowWindowAsync(native_target, SW_HIDE);
                let deadline = std::time::Instant::now() + Duration::from_secs(2);
                while IsWindowVisible(native_target).as_bool()
                    && std::time::Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                require(
                    !IsWindowVisible(native_target).as_bool(),
                    "Owned target was not hidden",
                )?;
                PostMessageW(Some(native_target), WM_APP, WPARAM(0), LPARAM(0))
                    .map_err(|e| e.to_string())?;
            }
            let focused = host
                .present(target, "focus", None)
                .map_err(|e| e.to_string())?;
            require(
                focused.visible && !focused.minimized && focused.mode == "independent",
                "Cross-thread display returned before the target was visible and independent",
            )?;
            witness("hidden_window_focus_waits_for_owned_target_queue");
            let original_style = unsafe { GetWindowLongPtrW(native_target, GWL_STYLE) };
            let original_exstyle = unsafe { GetWindowLongPtrW(native_target, GWL_EXSTYLE) };
            let original_owner = unsafe { GetWindow(native_target, GW_OWNER).unwrap_or_default() };
            let child_hwnd =
                unsafe { GetDlgItem(Some(native_target), 301).map_err(|e| e.to_string())? };
            let child_identity = super::software::fixture_candidate(child_hwnd.0 as usize)
                .map_err(|e| e.to_string())?
                .identity;
            let child_fallback = host
                .present(&child_identity, "embedded", Some(viewport))
                .map_err(|e| e.to_string())?;
            require(
                !child_fallback.embedding_supported
                    && unsafe { GetAncestor(child_hwnd, GA_PARENT) == native_target },
                "An existing foreign child was reparented",
            )?;
            witness("foreign_child_embedding_rejected_without_mutation");
            require(
                before.embedding_supported,
                "Controlled matching-DPI window is not embeddable",
            )?;
            let check_input = std::env::args()
                .any(|a| matches!(a.as_str(), "--check-input" | "--check-wechat-input"));
            if check_input {
                // This interactive scenario starts with the workbench in front. The other
                // overlapping, owned fixture is only needed for the discovery and DPI checks.
                let other = HWND(
                    mismatched
                        .hwnd
                        .parse::<usize>()
                        .map_err(|e| e.to_string())? as *mut _,
                );
                unsafe {
                    let _ = ShowWindowAsync(other, SW_HIDE);
                    super::input::activate_fixture_host(HWND(parent as *mut _))?;
                }
            }
            let embedded = host
                .present(target, "embedded", Some(viewport))
                .map_err(|e| e.to_string())?;
            require(
                embedded.mode == "embedded",
                "Native window was not embedded",
            )?;
            unsafe {
                require(
                        GetAncestor(native_target, GA_PARENT) == GetDesktopWindow()
                            && GetWindow(native_target, GW_OWNER).unwrap_or_default().is_invalid()
                            && GetWindowLongPtrW(native_target, GWL_STYLE) == original_style,
                        "Mount joined the target to the host input queue or changed its top-level style",
                    )?;
            }
            witness("real_cross_process_embed");
            if check_input {
                unsafe {
                    super::input::verify_edit(
                        native_target,
                        child.id(),
                        b'A' as u16,
                        &[b'A' as u16],
                    )?;
                }
                witness("generic_top_level_input_while_mounted");
            }
            require(
                host.present(
                    target,
                    "embedded",
                    Some(Viewport {
                        x: -1,
                        y: 0,
                        width: 600,
                        height: 350,
                    }),
                )
                .is_err(),
                "Invalid viewport was accepted",
            )?;
            witness("invalid_viewport_rejected");
            let state = host
                .present(target, "independent", None)
                .map_err(|e| e.to_string())?;
            require(
                state.mode == "independent",
                "Window was not restored independently",
            )?;
            witness("restore_same_window");
            if check_input {
                unsafe {
                    super::input::verify_edit(
                        native_target,
                        child.id(),
                        b'B' as u16,
                        &[b'A' as u16, b'B' as u16],
                    )?;
                }
                witness("generic_mouse_and_keyboard_immediately_after_detach");
            }
            require(
                unsafe {
                    GetAncestor(native_target, GA_PARENT) == GetDesktopWindow()
                        && GetWindowLongPtrW(native_target, GWL_STYLE) == original_style
                        && GetWindowLongPtrW(native_target, GWL_EXSTYLE) == original_exstyle
                        && GetWindow(native_target, GW_OWNER).unwrap_or_default() == original_owner
                },
                "Independent restore did not preserve real desktop parent, styles and owner",
            )?;
            witness("restore_desktop_parent_styles_and_owner_verified");
            eprintln!("software_fixture_stage:before_uia_observe");
            let controls = host
                .observe(target, "fixture-observation", false)
                .map_err(|e| e.to_string())?;
            require(
                controls.len() <= 64
                    && controls
                        .iter()
                        .all(|c| !c.is_password && c.automation_id != "303"),
                "Password or unbounded controls were exposed",
            )?;
            let button = controls
                .iter()
                .find(|c| c.can_invoke && c.name == "Apply")
                .ok_or("No structured button pattern exposed")?;
            let cancellation = std::sync::atomic::AtomicBool::new(true);
            let cancelled = host
                .control(target, button, "invoke", None, &cancellation)
                .expect_err("Cancelled action must not dispatch");
            require(
                cancelled.code == "cancelled",
                "Pre-cancelled operation was not rejected",
            )?;
            witness("cancelled_uia_action_rejected");
            cancellation.store(false, std::sync::atomic::Ordering::Release);
            let edit = controls
                .iter()
                .find(|c| c.automation_id == "302" && c.can_set_value)
                .ok_or("No writable edit control")?;
            host.control(target, edit, "set_value", Some("draft 2026"), &cancellation)
                .map_err(|e| e.to_string())?;
            require(
                controls.iter().all(|c| c.visible_text.is_none()),
                "Content returned without text permission",
            )?;
            let content = host
                .observe(target, "fixture-approved-content", true)
                .map_err(|e| e.to_string())?;
            require(
                content.iter().any(|c| {
                    c.automation_id == "302"
                        && c.visible_text
                            .as_deref()
                            .is_some_and(|s| s.contains("draft 2026"))
                }),
                "Approved editor text was not observed",
            )?;
            if std::env::args().any(|arg| arg == "--check-ocr") {
                let text = host.recognize_window(target).map_err(|e| e.to_string())?;
                let compact: String = text
                    .to_lowercase()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                require(
                    compact.contains("draft2026") || compact.contains("wordwrap"),
                    "Window OCR missed both known labels",
                )?;
                witness("generic_own_window_ocr_readback");
            }
            let checkbox = content
                .iter()
                .find(|c| c.automation_id == "304" && c.toggle_state.as_deref() == Some("off"))
                .ok_or("No unchecked checkbox")?;
            for action in ["check", "check", "uncheck"] {
                host.control(target, checkbox, action, None, &cancellation)
                    .map_err(|e| e.to_string())?;
            }
            witness("generic_set_value_readback_and_checkbox_states");
            let invoked = host
                .control(target, button, "invoke", None, &cancellation)
                .expect_err("Invoke must not fabricate business verification");
            require(
                invoked.code == "result_unknown",
                "Invocation did not preserve unknown outcome",
            )?;
            witness("uia_invoke_unknown_no_fake_success");
            let mut changed = false;
            for _ in 0..20 {
                if host
                    .observe(target, "fixture-after-invoke", false)
                    .map_err(|e| e.to_string())?
                    .iter()
                    .any(|c| c.name == "Applied")
                {
                    changed = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            require(
                changed,
                "Controlled fixture did not observe the invoked button's new caption",
            )?;
            require(
                host.control(target, button, "invoke", None, &cancellation)
                    .is_err(),
                "Changed button identity was accepted",
            )?;
            witness("uia_stale_control_rejected");
            host.present(target, "embedded", Some(viewport))
                .map_err(|e| e.to_string())?;
            host.restore_embedded()?;
            require(
                host.verify(target).map_err(|e| e.to_string())?.mode == "independent",
                "Host hide did not restore window",
            )?;
            witness("host_hide_restore");
            host.suspend_embedding()?;
            require(
                host.present(target, "embedded", Some(viewport)).is_err(),
                "Presentation bypassed host suspension",
            )?;
            host.set_parent(parent as u64)?;
            witness("suspended_host_rejects_embed_until_resume");
            let seal_host = std::sync::Arc::new(NativeSoftwareHost::new());
            seal_host.set_parent(parent as u64)?;
            let sealed_controls = seal_host
                .observe(target, "fixture-before-seal", false)
                .map_err(|e| e.to_string())?;
            let sealed_button = sealed_controls
                .iter()
                .find(|c| c.name == "Applied")
                .ok_or("No already-applied fixture button")?
                .clone();
            let (reached_tx, reached_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            seal_host.fixture_pause_next_control(reached_tx, resume_rx);
            let control_host = std::sync::Arc::clone(&seal_host);
            let control_target = target.clone();
            let worker = std::thread::spawn(move || {
                control_host.control(
                    &control_target,
                    &sealed_button,
                    "invoke",
                    None,
                    &std::sync::atomic::AtomicBool::new(false),
                )
            });
            reached_rx
                .recv_timeout(Duration::from_secs(4))
                .map_err(|_| "Control did not reach the pre-dispatch fixture gate")?;
            require(
                seal_host
                    .shutdown()
                    .expect_err("Active control must block parent destruction")
                    .code
                    == "native_work_in_progress",
                "Shutdown failed to detect native work",
            )?;
            resume_tx.send(()).map_err(|e| e.to_string())?;
            let cancelled = worker
                .join()
                .map_err(|_| "Native control worker panicked")?
                .expect_err("Sealed host must cancel an admitted operation before dispatch");
            require(
                cancelled.code == "cancelled",
                "In-flight control did not respect shutdown seal",
            )?;
            let after_seal = seal_host
                .observe(target, "fixture-after-seal", false)
                .map_err(|e| e.to_string())?;
            require(
                after_seal.iter().any(|c| c.name == "Applied")
                    && !after_seal.iter().any(|c| c.name == "Applied again"),
                "Shutdown raced past an actual invocation",
            )?;
            seal_host.shutdown().map_err(|e| e.to_string())?;
            witness("shutdown_cancels_inflight_uia_before_dispatch");
            host.shutdown().map_err(|e| e.to_string())?;
            let (send, receive) = mpsc::channel();
            let late_host = std::sync::Arc::clone(&host);
            let late_target = target.clone();
            let late = std::thread::spawn(move || {
                receive.recv_timeout(Duration::from_secs(2)).unwrap();
                late_host.present(&late_target, "embedded", Some(viewport))
            });
            send.send(()).map_err(|e| e.to_string())?;
            let rejected = late
                .join()
                .map_err(|_| "Late presentation worker panicked")?
                .expect_err("Sealed host must reject late presentation");
            require(
                rejected.code == "software_host_closed",
                "Late presentation bypassed shutdown seal",
            )?;
            witness("shutdown_rejects_late_embed");
            require(
                child.try_wait().map_err(|e| e.to_string())?.is_none(),
                "Shutdown terminated the attached process",
            )?;
            witness("shutdown_never_kills_attached_process");
            Ok(())
        })();
        let _ = host.shutdown();
        // Only these two explicitly spawned fixture processes/windows are cleaned up.
        if let Ok(windows) = host.discover(&Registration {
            id: "cleanup".into(),
            revision: 1,
            config: RegistrationInput {
                name: "Fixture".into(),
                adapter: "generic_fixture".into(),
                executable: std::env::current_exe()
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .to_string(),
                workspace_id: None,
                allow_observe: false,
                allow_content: false,
                allow_window_ocr: false,
                allow_actions: false,
            },
        }) {
            for window in windows.into_iter().filter(|w| w.identity.pid == child.id()) {
                if let Ok(raw) = window.identity.hwnd.parse::<usize>() {
                    unsafe {
                        let _ =
                            PostMessageW(Some(HWND(raw as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            }
        }
        for _ in 0..30 {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(parent as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        let _ = thread.join();
        result
    }
}
