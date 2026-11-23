use super::{
    automation, dock,
    geometry::physical_viewport,
    identity::{self, error},
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        HiDpi::{AreDpiAwarenessContextsEqual, GetWindowDpiAwarenessContext},
        WindowsAndMessaging::*,
    },
};
use yilian_backend::software::{
    Registration, SoftwareControl, SoftwareError, SoftwareHost, Viewport, WindowCandidate,
    WindowIdentity, WindowState,
};

#[derive(Clone)]
struct Original {
    identity: WindowIdentity,
    placement: WINDOWPLACEMENT,
    visible: bool,
    dock_viewport: Viewport,
}
pub struct NativeSoftwareHost {
    parent: AtomicU64,
    mounted: Mutex<HashMap<String, Original>>,
    closing: AtomicBool,
    active_changes: AtomicUsize,
    dock_sync_pending: AtomicBool,
    embedding_paused: AtomicBool,
    #[cfg(all(debug_assertions, feature = "native-software-fixture"))]
    dispatch_pause: Mutex<Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>>,
}
struct NativeChange<'a>(&'a AtomicUsize);
impl Drop for NativeChange<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn wait_for_window_state(
    target: &WindowIdentity,
    matches: impl Fn(HWND) -> bool,
) -> Result<bool, SoftwareError> {
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        if matches(identity::verify(target)?) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

impl NativeSoftwareHost {
    pub fn new() -> Self {
        Self {
            parent: AtomicU64::new(0),
            mounted: Mutex::new(HashMap::new()),
            closing: AtomicBool::new(false),
            active_changes: AtomicUsize::new(0),
            dock_sync_pending: AtomicBool::new(false),
            embedding_paused: AtomicBool::new(false),
            #[cfg(all(debug_assertions, feature = "native-software-fixture"))]
            dispatch_pause: Mutex::new(None),
        }
    }
    fn begin_change(&self) -> Result<NativeChange<'_>, SoftwareError> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(error(
                "software_host_closed",
                "Native software host is quiescing; no new action is accepted",
            ));
        }
        self.active_changes.fetch_add(1, Ordering::SeqCst);
        let guard = NativeChange(&self.active_changes);
        if self.closing.load(Ordering::SeqCst) {
            return Err(error(
                "software_host_closed",
                "Native software host is quiescing; no new action is accepted",
            ));
        }
        Ok(guard)
    }
    /// Only called by trusted desktop bootstrap/window lifecycle, never web payloads.
    pub fn set_parent(&self, hwnd: u64) -> Result<(), String> {
        if !unsafe { IsWindow(Some(HWND(hwnd as usize as *mut _))).as_bool() } {
            return Err("Invalid native host window".into());
        }
        if self.parent.load(Ordering::Acquire) != hwnd {
            self.restore_embedded()?;
        }
        self.parent.store(hwnd, Ordering::Release);
        let _mounts = self
            .mounted
            .lock()
            .map_err(|_| "Software mount state unavailable")?;
        if !self.closing.load(Ordering::SeqCst)
            && unsafe {
                IsWindowVisible(HWND(hwnd as usize as *mut _)).as_bool()
                    && !IsIconic(HWND(hwnd as usize as *mut _)).as_bool()
            }
        {
            self.embedding_paused.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    /// Moving or activating the host does not generate a DOM resize. Align using the
    /// current origin on a worker so the host message loop never waits for a target.
    pub fn host_moved_or_activated(self: &std::sync::Arc<Self>) {
        if self.mounted.try_lock().is_ok_and(|m| m.is_empty())
            || self.dock_sync_pending.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mounts = this.mounted.lock();
            this.dock_sync_pending.store(false, Ordering::Release);
            let (Ok(mounts), Ok(parent)) = (mounts, this.parent()) else {
                return;
            };
            if this.embedding_paused.load(Ordering::Acquire)
                || this.closing.load(Ordering::Acquire)
                || unsafe { IsIconic(parent).as_bool() || !IsWindowVisible(parent).as_bool() }
            {
                return;
            }
            for original in mounts.values() {
                let viewport = original.dock_viewport;
                if let Ok(hwnd) = identity::verify(&original.identity) {
                    if unsafe { IsIconic(hwnd).as_bool() } {
                        continue;
                    }
                    if let Err(e) = dock::align(parent, hwnd, viewport, false) {
                        tracing::debug!(%e, "Dock alignment deferred");
                    }
                }
            }
        });
    }
    pub fn suspend_embedding(&self) -> Result<(), String> {
        self.embedding_paused.store(true, Ordering::SeqCst);
        self.restore_embedded()
    }
    #[cfg(all(debug_assertions, feature = "native-software-fixture"))]
    pub fn fixture_pause_next_control(
        &self,
        reached: std::sync::mpsc::Sender<()>,
        resume: std::sync::mpsc::Receiver<()>,
    ) {
        *self.dispatch_pause.lock().expect("fixture dispatch lock") = Some((reached, resume));
    }
    fn before_dispatch(&self) -> Result<(), SoftwareError> {
        #[cfg(all(debug_assertions, feature = "native-software-fixture"))]
        if let Some((reached, resume)) = self
            .dispatch_pause
            .lock()
            .map_err(|_| error("state_unavailable", "Dispatch state unavailable"))?
            .take()
        {
            reached
                .send(())
                .map_err(|_| error("cancelled", "Fixture dispatch owner closed"))?;
            resume
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| error("cancelled", "Fixture dispatch was not released"))?;
        }
        Ok(())
    }
    pub fn restore_embedded(&self) -> Result<(), String> {
        let targets = self
            .mounted
            .lock()
            .map_err(|_| "Software mount state unavailable")?
            .values()
            .map(|o| o.identity.clone())
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for target in targets {
            if let Err(e) = self.release(&target) {
                if e.code != "window_closed" {
                    errors.push(e.to_string());
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
    fn parent(&self) -> Result<HWND, SoftwareError> {
        let hwnd = HWND(self.parent.load(Ordering::Acquire) as usize as *mut _);
        if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
            return Err(error(
                "host_window_unavailable",
                "Open the main desktop window first",
            ));
        }
        Ok(hwnd)
    }
    fn state(
        &self,
        target: &WindowIdentity,
        reason: Option<String>,
    ) -> Result<WindowState, SoftwareError> {
        let hwnd = identity::verify(target)?;
        let embedded = self
            .mounted
            .lock()
            .map_err(|_| error("state_unavailable", "Software mount lock unavailable"))?
            .contains_key(&target.hwnd);
        let same_dpi = self.parent().ok().is_some_and(|parent| unsafe {
            AreDpiAwarenessContextsEqual(
                GetWindowDpiAwarenessContext(parent),
                GetWindowDpiAwarenessContext(hwnd),
            )
            .as_bool()
        });
        let foreign_child =
            unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } & WS_CHILD.0 as isize != 0 && !embedded;
        let owned_popup = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } & WS_CHILD.0 as isize == 0
            && unsafe { GetWindow(hwnd, GW_OWNER).is_ok() };
        let supported = same_dpi && !foreign_child && !owned_popup;
        Ok(WindowState { identity: target.clone(), mode: if embedded { "embedded" } else { "independent" }.into(),
            visible: unsafe { IsWindowVisible(hwnd).as_bool() }, focused: unsafe { GetForegroundWindow() == hwnd }, minimized: unsafe { IsIconic(hwnd).as_bool() },
            embedding_supported: supported,
            reason: reason.or_else(|| (!supported).then(|| if foreign_child || owned_popup { "Owned popups and existing foreign child windows cannot be safely reparented; independent mode retained".into() } else { "Native host or matching DPI awareness is unavailable; independent mode retained".into() })) })
    }
    fn restore(&self, original: &Original) -> Result<(), SoftwareError> {
        let hwnd = identity::verify(&original.identity)?;
        dock::restore(hwnd, &original.placement, original.visible)?;
        if wait_for_window_state(&original.identity, |hwnd| {
            dock::restored(hwnd, &original.placement, original.visible)
        })? {
            Ok(())
        } else {
            Err(SoftwareError::unknown(
                "尚未确认窗口恢复到接入前的位置和显示状态",
            ))
        }
    }
}
impl Default for NativeSoftwareHost {
    fn default() -> Self {
        Self::new()
    }
}
impl SoftwareHost for NativeSoftwareHost {
    fn available(&self) -> bool {
        !self.closing.load(Ordering::SeqCst)
    }
    fn discover(&self, registration: &Registration) -> Result<Vec<WindowCandidate>, SoftwareError> {
        identity::discover(
            &registration.config.executable,
            registration.config.adapter == "wechat",
        )
    }
    fn launch(&self, registration: &Registration) -> Result<WindowCandidate, SoftwareError> {
        let _change = self.begin_change()?;
        let executable = identity::canonical_executable(&registration.config.executable)?;
        if self.closing.load(Ordering::SeqCst) {
            return Err(error(
                "software_host_closed",
                "Host closed before application launch",
            ));
        }
        let child = std::process::Command::new(executable)
            .spawn()
            .map_err(|e| error("launch_failed", e))?;
        let pid = child.id();
        let process_started_at = {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::{
                Foundation::{FILETIME, HANDLE},
                System::Threading::GetProcessTimes,
            };
            let (mut created, mut exited, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            unsafe {
                GetProcessTimes(
                    HANDLE(child.as_raw_handle()),
                    &mut created,
                    &mut exited,
                    &mut kernel,
                    &mut user,
                )
            }
            .map_err(|e| {
                SoftwareError::unknown(format!(
                    "Application was launched but its process lifetime could not be recorded: {e}"
                ))
            })?;
            ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64
        };
        drop(child); // Child ownership is NOT process-termination authority.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            let mut candidates = self
                .discover(registration)
                .map_err(|e| {
                    SoftwareError::unknown(format!(
                        "Application was launched but window discovery failed: {e}"
                    ))
                })?
                .into_iter()
                .filter(|w| {
                    w.identity.pid == pid && w.identity.process_started_at == process_started_at
                })
                .collect::<Vec<_>>();
            if candidates.len() == 1 {
                return Ok(candidates.remove(0));
            }
            if candidates.len() > 1 {
                return Err(SoftwareError::unknown("Application started with several windows; inspect candidates and select one. Do not launch again."));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(SoftwareError::unknown("Launch returned but no unique window was identified; the process may still be running. Inspect before retrying."))
    }
    fn verify(&self, target: &WindowIdentity) -> Result<WindowState, SoftwareError> {
        self.state(target, None)
    }
    fn monitor_process_alive(&self, target: &WindowIdentity) -> Result<bool, SoftwareError> {
        identity::process_alive(target)
    }
    fn monitor_notifications(
        &self,
        target: &WindowIdentity,
    ) -> yilian_backend::software::SoftwareNotificationSnapshot {
        super::notifications::snapshot(&target.executable)
    }
    fn monitor_window_events(
        &self,
        pid: u32,
        cancel: tokio_util::sync::CancellationToken,
        wake: std::sync::Arc<tokio::sync::Notify>,
    ) {
        super::monitor::watch(pid, cancel, wake);
    }
    fn present(
        &self,
        target: &WindowIdentity,
        mode: &str,
        viewport: Option<Viewport>,
    ) -> Result<WindowState, SoftwareError> {
        let hwnd = identity::verify(target)?;
        if matches!(mode, "independent" | "detached") {
            self.release(target)?;
            return self.state(target, None);
        }
        let _change = self.begin_change()?;
        if mode == "focus" {
            self.release(target)?;
            let hwnd = identity::verify(target)?;
            if self.closing.load(Ordering::SeqCst) {
                return Err(error(
                    "software_host_closed",
                    "Host closed before changing focus",
                ));
            }
            unsafe {
                if IsIconic(hwnd).as_bool() {
                    let _ = ShowWindowAsync(hwnd, SW_RESTORE);
                } else if !IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindowAsync(hwnd, SW_SHOW);
                }
                // Cross-thread ShowWindowAsync and foreground activation complete on
                // the target's message queue. Observe completion before the next step.
                if !wait_for_window_state(target, |window| {
                    IsWindowVisible(window).as_bool() && !IsIconic(window).as_bool()
                })? {
                    return Err(SoftwareError::unknown(
                        "The selected window did not finish becoming visible",
                    ));
                }
                let _ = SetForegroundWindow(hwnd);
            }
            // Windows can decline foreground activation even after the detached
            // window is visible. Report actual focus separately from presentation.
            return self.state(target, None);
        }
        if mode != "embedded" {
            return Err(error(
                "invalid_mode",
                "Expected embedded, independent or focus",
            ));
        }
        // Preserve the original input queue and owner throughout docking.
        let state = self.state(target, None)?;
        if !state.embedding_supported {
            return Ok(state);
        }
        let parent = self.parent()?;
        if hwnd == parent {
            return Err(error(
                "invalid_target",
                "The host window cannot mount itself",
            ));
        }
        let viewport = viewport.ok_or_else(|| {
            error(
                "viewport_required",
                "Embedding requires the visible content rectangle",
            )
        })?;
        physical_viewport(parent, viewport)?;
        let mut mounted = self
            .mounted
            .lock()
            .map_err(|_| error("state_unavailable", "Mount state unavailable"))?;
        if self.closing.load(Ordering::Acquire) {
            return Err(error(
                "software_host_closed",
                "Native software host is quiescing; no embedding is accepted",
            ));
        }
        if self.embedding_paused.load(Ordering::SeqCst)
            || !unsafe { IsWindowVisible(parent).as_bool() }
            || unsafe { IsIconic(parent).as_bool() }
        {
            return Err(error(
                "software_embedding_suspended",
                "The main window is hidden or minimizing; independent mode is retained",
            ));
        }
        if !mounted.contains_key(&target.hwnd) {
            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            unsafe {
                GetWindowPlacement(hwnd, &mut placement)
                    .map_err(|e| error("window_state_unavailable", e))?;
            }
            let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
            if style & WS_CHILD.0 as isize != 0 || unsafe { GetWindow(hwnd, GW_OWNER).is_ok() } {
                return Err(error(
                    "unsupported_embedding",
                    "Only an unowned top-level window can be docked",
                ));
            }
            let original = Original {
                identity: target.clone(),
                placement,
                visible: unsafe { IsWindowVisible(hwnd).as_bool() },
                dock_viewport: viewport,
            };
            mounted.insert(target.hwnd.clone(), original);
        }
        let original = mounted.get_mut(&target.hwnd).expect("mount recorded above");
        if original.identity != *target {
            return Err(error(
                "target_changed",
                "Mount belongs to another window lifetime",
            ));
        }
        original.dock_viewport = viewport;
        if unsafe { IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() } {
            unsafe {
                let _ = ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        let aligned = dock::align(parent, hwnd, viewport, true).and_then(|bounds| {
            wait_for_window_state(target, |window| dock::at_bounds(window, bounds))
        });
        let reason = match aligned {
            Ok(true) => None,
            Ok(false) => Some("窗口无法适应当前挂载区域，可放大工作台或选择独立显示。".into()),
            Err(e) => Some(format!("窗口定位失败：{e}")),
        };
        if reason.is_some() {
            self.restore(original)?;
            mounted.remove(&target.hwnd);
            drop(mounted);
            return self.state(target, reason);
        }
        drop(mounted);
        self.state(target, None)
    }
    fn release(&self, target: &WindowIdentity) -> Result<(), SoftwareError> {
        let mut mounted = self
            .mounted
            .lock()
            .map_err(|_| error("state_unavailable", "Mount state unavailable"))?;
        if let Some(original) = mounted.get(&target.hwnd).cloned() {
            if &original.identity != target {
                return Err(error(
                    "target_changed",
                    "Mount belongs to a different process lifetime",
                ));
            }
            if let Err(e) = identity::verify(target) {
                if matches!(e.code.as_str(), "window_closed" | "target_changed") {
                    mounted.remove(&target.hwnd);
                    return Err(error(
                        "window_closed",
                        "Original window closed or changed; no replacement window was modified",
                    ));
                }
                // Access loss is not proof of disappearance: retain restoration ownership.
                return Err(e);
            }
            self.restore(&original)?;
            mounted.remove(&target.hwnd);
        }
        Ok(())
    }
    fn write_wechat_draft(
        &self,
        target: &WindowIdentity,
        group: Option<&str>,
        text: &str,
        replace: bool,
        cancel: &AtomicBool,
    ) -> Result<(), SoftwareError> {
        let _change = self.begin_change()?;
        super::chat_draft::write(target, group, text, replace, cancel, &self.closing)
    }
    fn recognize_window(&self, target: &WindowIdentity) -> Result<String, SoftwareError> {
        let bytes = super::chat_capture::capture(target).map_err(|e| {
            if e.code == "wechat_capture_failed" {
                error("window_capture_failed", "无法读取这个窗口的画面")
            } else {
                e
            }
        })?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|e| error("window_capture_failed", e))?;
        super::chat_capture::recognize_image(image).map_err(|e| error("window_text_unavailable", e))
    }
    fn wechat_frame_png(&self, target: &WindowIdentity) -> Result<Vec<u8>, SoftwareError> {
        super::chat_capture::capture(target)
    }
    fn wechat_conversation(
        &self,
        target: &WindowIdentity,
    ) -> Result<yilian_backend::software::WechatConversation, SoftwareError> {
        automation::wechat::read(target)
    }
    fn observe(
        &self,
        target: &WindowIdentity,
        observation_id: &str,
        read_content: bool,
    ) -> Result<Vec<SoftwareControl>, SoftwareError> {
        identity::verify(target)?;
        automation::observe(target, observation_id, read_content)
    }
    fn control(
        &self,
        target: &WindowIdentity,
        control: &SoftwareControl,
        action: &str,
        value: Option<&str>,
        cancellation: &AtomicBool,
    ) -> Result<(), SoftwareError> {
        let _change = self.begin_change()?;
        identity::verify(target)?;
        automation::control(
            target,
            control,
            action,
            value,
            cancellation,
            &self.closing,
            &|| self.before_dispatch(),
        )
    }
    fn shutdown(&self) -> Result<(), SoftwareError> {
        self.closing.store(true, Ordering::SeqCst);
        if self.active_changes.load(Ordering::SeqCst) != 0 {
            return Err(error("native_work_in_progress", "Native software work is still finishing; do not destroy the parent window, retry restoration after completion"));
        }
        self.restore_embedded()
            .map_err(|e| error("restore_failed", e))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_seals_before_observing_active_native_work() {
        let host = NativeSoftwareHost::new();
        let in_flight = host.begin_change().unwrap();
        assert_eq!(host.shutdown().unwrap_err().code, "native_work_in_progress");
        assert!(host.begin_change().is_err());
        drop(in_flight);
        host.shutdown().unwrap();
        assert!(host.begin_change().is_err());
        host.restore_embedded().unwrap();
    }
}
