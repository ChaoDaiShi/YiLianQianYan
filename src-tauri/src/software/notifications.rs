use std::path::Path;
use windows::{
    core::GUID,
    Win32::{
        System::{
            Com::CoTaskMemFree,
            WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
        },
        UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    },
    UI::Notifications::{
        Management::{UserNotificationListener, UserNotificationListenerAccessStatus},
        NotificationKinds,
    },
};
use yilian_backend::software::SoftwareNotificationSnapshot;

/// Match the OS source identity to the registered executable. Unpackaged
/// applications can be identified by a path relative to a Windows known folder.
fn source_matches(app_id: &str, executable: &str) -> bool {
    let executable = executable.strip_prefix(r"\\?\").unwrap_or(executable);
    if app_id.eq_ignore_ascii_case(executable) {
        return true;
    }
    let Some((guid, relative)) = app_id.strip_prefix('{').and_then(|id| id.split_once(r"}\"))
    else {
        return false;
    };
    let Ok(guid) = GUID::try_from(guid) else {
        return false;
    };
    let directory = unsafe {
        let Ok(path) = SHGetKnownFolderPath(&guid, KF_FLAG_DEFAULT, None) else {
            return false;
        };
        let directory = path.to_string();
        CoTaskMemFree(Some(path.0.cast()));
        directory
    };
    directory.is_ok_and(|directory| {
        Path::new(&directory)
            .join(relative)
            .to_string_lossy()
            .eq_ignore_ascii_case(executable)
    })
}

pub(super) fn snapshot(executable: &str) -> SoftwareNotificationSnapshot {
    if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
        return SoftwareNotificationSnapshot::Unavailable;
    }
    let result = read_ids(executable).unwrap_or(SoftwareNotificationSnapshot::Unavailable);
    unsafe { RoUninitialize() };
    result
}

fn read_ids(executable: &str) -> windows::core::Result<SoftwareNotificationSnapshot> {
    let listener = UserNotificationListener::Current()?;
    if listener.GetAccessStatus()? != UserNotificationListenerAccessStatus::Allowed {
        return Ok(SoftwareNotificationSnapshot::AccessRequired);
    }
    let notifications = listener
        .GetNotificationsAsync(NotificationKinds::Toast)?
        .get()?;
    if listener.GetAccessStatus()? != UserNotificationListenerAccessStatus::Allowed {
        return Ok(SoftwareNotificationSnapshot::AccessRequired);
    }
    let mut ids = Vec::new();
    for item in notifications {
        let Ok(app_id) = item.AppInfo().and_then(|app| app.AppUserModelId()) else {
            continue;
        };
        if source_matches(&app_id.to_string_lossy(), executable) {
            if let Ok(id) = item.Id() {
                ids.push(id);
            }
        }
    }
    Ok(SoftwareNotificationSnapshot::Available(ids))
}
