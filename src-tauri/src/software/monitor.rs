use std::{cell::OnceCell, sync::Arc};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK},
        WindowsAndMessaging::*,
    },
};

thread_local! {
    static WAKE: OnceCell<Arc<Notify>> = const { OnceCell::new() };
}

unsafe extern "system" fn on_window_event(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if !hwnd.is_invalid() && object == OBJID_WINDOW.0 && child == 0 {
        WAKE.with(|slot| {
            if let Some(wake) = slot.get() {
                wake.notify_one();
            }
        });
    }
}

pub(super) fn watch(pid: u32, cancel: CancellationToken, wake: Arc<Notify>) {
    let _ = std::thread::Builder::new()
        .name("software-window-events".into())
        .spawn(move || unsafe {
            WAKE.with(|slot| {
                let _ = slot.set(wake);
            });
            let hook = SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_DESTROY,
                None,
                Some(on_window_event),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            if hook.is_invalid() {
                return;
            }
            let mut message = MSG::default();
            while !cancel.is_cancelled() {
                MsgWaitForMultipleObjectsEx(None, 500, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                while !cancel.is_cancelled()
                    && PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool()
                {
                    DispatchMessageW(&message);
                }
            }
            let _ = UnhookWinEvent(hook);
        });
}
