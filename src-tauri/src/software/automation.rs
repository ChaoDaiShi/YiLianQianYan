//! Window-scoped UI Automation with separately delegated visible text.
use super::identity::{self, error};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows::{
    core::{Interface, BSTR},
    Win32::{
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
                COINIT_MULTITHREADED,
            },
            Ole::{
                SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetLBound,
                SafeArrayGetUBound, SafeArrayGetVartype, SafeArrayUnaccessData,
            },
            Variant::VT_I4,
        },
        UI::Accessibility::*,
    },
};
use yilian_backend::software::{SoftwareControl, SoftwareError, WindowIdentity};

struct Apartment;
const PROVIDER_TIMEOUT_MS: u32 = 1500;
struct Budget {
    deadline: Instant,
}
impl Budget {
    fn new() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(6),
        }
    }
    fn check(&self, phase: &'static str) -> Result<(), SoftwareError> {
        stage(phase);
        if Instant::now() >= self.deadline {
            Err(error(
                "uia_timeout",
                format!("UI Automation exceeded its bounded read/dispatch budget before {phase}"),
            ))
        } else {
            Ok(())
        }
    }
}
fn stage(phase: &'static str) {
    #[cfg(all(debug_assertions, feature = "native-software-fixture"))]
    eprintln!("software_uia_stage:{phase}");
    tracing::debug!(phase, "Native UI Automation stage");
}
impl Apartment {
    fn enter() -> Result<Self, SoftwareError> {
        stage("com_initialize");
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| error("uia_thread_unavailable", e))?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        stage("com_uninitialize");
        unsafe {
            CoUninitialize();
        }
    }
}
mod helpers;
mod patterns;
pub(super) mod wechat;
use helpers::{optional_child, optional_pattern};
use patterns::{describe_patterns, read_visible_text, set_pattern_state, verify_readback};
fn client(budget: &Budget) -> Result<IUIAutomation, SoftwareError> {
    budget.check("com_create_uia8")?;
    unsafe {
        let automation: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| error("uia_unavailable", e))?;
        budget.check("configure_provider_timeouts")?;
        automation
            .SetConnectionTimeout(PROVIDER_TIMEOUT_MS)
            .map_err(|e| error("uia_timeout_configuration_failed", e))?;
        automation
            .SetTransactionTimeout(PROVIDER_TIMEOUT_MS)
            .map_err(|e| error("uia_timeout_configuration_failed", e))?;
        automation
            .SetAutoSetFocus(false)
            .map_err(|e| error("uia_focus_configuration_failed", e))?;
        if let Ok(modern) = automation.cast::<IUIAutomation6>() {
            modern
                .SetConnectionRecoveryBehavior(ConnectionRecoveryBehaviorOptions_Disabled)
                .map_err(|e| error("uia_timeout_configuration_failed", e))?;
        }
        if automation
            .ConnectionTimeout()
            .map_err(|e| error("uia_timeout_configuration_failed", e))?
            != PROVIDER_TIMEOUT_MS
            || automation
                .TransactionTimeout()
                .map_err(|e| error("uia_timeout_configuration_failed", e))?
                != PROVIDER_TIMEOUT_MS
        {
            return Err(error(
                "uia_timeout_configuration_failed",
                "Provider timeout settings were not retained",
            ));
        }
        budget.check("provider_timeouts_verified")?;
        automation.cast().map_err(|e| error("uia_unavailable", e))
    }
}
unsafe fn cache_request(
    automation: &IUIAutomation,
    budget: &Budget,
) -> Result<IUIAutomationCacheRequest, SoftwareError> {
    budget.check("metadata_cache_request")?;
    let cache = automation
        .CreateCacheRequest()
        .map_err(|e| error("uia_cache_unavailable", e))?;
    for property in [
        UIA_ProcessIdPropertyId,
        UIA_ControlTypePropertyId,
        UIA_IsPasswordPropertyId,
        UIA_IsEnabledPropertyId,
        UIA_IsOffscreenPropertyId,
        UIA_AutomationIdPropertyId,
    ] {
        cache
            .AddProperty(property)
            .map_err(|e| error("uia_cache_unavailable", e))?;
    }
    cache
        .SetTreeScope(TreeScope_Element)
        .map_err(|e| error("uia_cache_unavailable", e))?;
    Ok(cache)
}
unsafe fn runtime_id(
    element: &IUIAutomationElement,
    budget: &Budget,
) -> Result<Vec<i32>, SoftwareError> {
    budget.check("control_runtime_id")?;
    let array = element
        .GetRuntimeId()
        .map_err(|e| error("control_identity_unavailable", e))?;
    if array.is_null() {
        return Err(error(
            "control_identity_unavailable",
            "UIA runtime identity is absent",
        ));
    }
    let result = (|| {
        if SafeArrayGetDim(array) != 1
            || SafeArrayGetVartype(array).map_err(|e| error("control_identity_unavailable", e))?
                != VT_I4
        {
            return Err(error(
                "control_identity_unavailable",
                "Invalid UIA identity dimensions or type",
            ));
        }
        let lower =
            SafeArrayGetLBound(array, 1).map_err(|e| error("control_identity_unavailable", e))?;
        let upper =
            SafeArrayGetUBound(array, 1).map_err(|e| error("control_identity_unavailable", e))?;
        let length = upper as i64 - lower as i64 + 1;
        if !(1..=128).contains(&length) {
            return Err(error(
                "control_identity_unavailable",
                "UIA identity is outside limits",
            ));
        }
        let mut raw = std::ptr::null_mut();
        SafeArrayAccessData(array, &mut raw)
            .map_err(|e| error("control_identity_unavailable", e))?;
        let values = std::slice::from_raw_parts(raw as *const i32, length as usize).to_vec();
        SafeArrayUnaccessData(array).map_err(|e| error("control_identity_unavailable", e))?;
        Ok(values)
    })();
    let _ = SafeArrayDestroy(array);
    result
}
#[allow(non_upper_case_globals)] // Windows binding constants retain their native API names.
unsafe fn describe(
    element: &IUIAutomationElement,
    path: &[u32],
    nonce: &str,
    pid: u32,
    budget: &Budget,
    request: &IUIAutomationCacheRequest,
) -> Result<SoftwareControl, SoftwareError> {
    budget.check("refresh_control_metadata_cache")?;
    let cached = element
        .BuildUpdatedCache(request)
        .map_err(|e| error("uia_provider_failed", e))?;
    budget.check("control_process_id")?;
    if cached
        .CachedProcessId()
        .map_err(|e| error("uia_provider_failed", e))? as u32
        != pid
    {
        return Err(error(
            "foreign_control",
            "Control is owned by a different process",
        ));
    }
    budget.check("control_type")?;
    let role = cached
        .CachedControlType()
        .map_err(|e| error("uia_provider_failed", e))?;
    budget.check("control_password_flag")?;
    let password = cached
        .CachedIsPassword()
        .map_err(|e| error("uia_provider_failed", e))?
        .as_bool();
    if password {
        return Err(error("sensitive_control", "Password controls are excluded"));
    }
    budget.check("control_availability")?;
    let enabled = cached
        .CachedIsEnabled()
        .map_err(|e| error("uia_provider_failed", e))?
        .as_bool();
    budget.check("control_offscreen_flag")?;
    let enabled = enabled
        && !cached
            .CachedIsOffscreen()
            .map_err(|e| error("uia_provider_failed", e))?
            .as_bool();
    budget.check("control_automation_id")?;
    let automation_id = cached
        .CachedAutomationId()
        .map_err(|e| error("uia_provider_failed", e))?
        .to_string()
        .chars()
        .take(160)
        .collect::<String>();
    // Text, document, edits and list entries can expose chat bodies via Name.
    let safe_caption = matches!(
        role,
        UIA_ButtonControlTypeId
            | UIA_CheckBoxControlTypeId
            | UIA_ComboBoxControlTypeId
            | UIA_MenuItemControlTypeId
            | UIA_RadioButtonControlTypeId
            | UIA_TabItemControlTypeId
    );
    let name = if safe_caption {
        budget.check("control_safe_caption")?;
        element
            .CurrentName()
            .map_err(|e| error("uia_provider_failed", e))?
            .to_string()
            .chars()
            .take(120)
            .collect()
    } else {
        String::new()
    };
    let file_input = ["filename", "file_name", "filepath", "file_path"]
        .iter()
        .any(|s| automation_id.to_ascii_lowercase().contains(s));
    budget.check("control_patterns")?;
    let can_invoke = if enabled
        && matches!(
            role,
            UIA_ButtonControlTypeId | UIA_MenuItemControlTypeId | UIA_HyperlinkControlTypeId
        ) {
        optional_pattern(
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId),
        )?
        .is_some()
    } else {
        false
    };
    budget.check("control_value_pattern")?;
    let can_set_value = if enabled
        && matches!(role, UIA_EditControlTypeId | UIA_DocumentControlTypeId)
        && !file_input
    {
        if let Some(pattern) = optional_pattern(
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId),
        )? {
            budget.check("control_readonly_flag")?;
            !pattern
                .CurrentIsReadOnly()
                .map_err(|e| error("uia_provider_failed", e))?
                .as_bool()
        } else {
            false
        }
    } else {
        false
    };
    let (selected, toggle_state, expand_state) = describe_patterns(element, role)?;
    let runtime = runtime_id(element, budget)?;
    let fingerprint = format!("{:?}|{}|{}|{}", runtime, role.0, automation_id, name);
    let mut hasher = DefaultHasher::new();
    (nonce, path, &fingerprint).hash(&mut hasher);
    Ok(SoftwareControl {
        id: format!("control-{:016x}", hasher.finish()),
        name,
        role: match role {
            UIA_ButtonControlTypeId => "button",
            UIA_EditControlTypeId => "edit",
            UIA_WindowControlTypeId => "window",
            UIA_TextControlTypeId => "text",
            UIA_DocumentControlTypeId => "document",
            UIA_CheckBoxControlTypeId => "checkbox",
            UIA_RadioButtonControlTypeId => "radio",
            UIA_TabControlTypeId => "tabs",
            UIA_TabItemControlTypeId => "tab",
            UIA_MenuControlTypeId | UIA_MenuBarControlTypeId => "menu",
            UIA_MenuItemControlTypeId => "menu_item",
            UIA_ComboBoxControlTypeId => "combo_box",
            UIA_TreeControlTypeId => "tree",
            UIA_TreeItemControlTypeId => "tree_item",
            UIA_ListControlTypeId => "list",
            UIA_ListItemControlTypeId => "list_item",
            UIA_DataItemControlTypeId => "data_item",
            UIA_HyperlinkControlTypeId => "link",
            UIA_ToolBarControlTypeId => "toolbar",
            UIA_GroupControlTypeId => "group",
            UIA_PaneControlTypeId => "pane",
            _ => "control",
        }
        .into(),
        automation_id,
        enabled,
        can_invoke,
        can_set_value,
        selected,
        toggle_state,
        expand_state,
        visible_text: None,
        is_password: false,
        fingerprint,
        path: path.to_vec(),
    })
}
pub(super) fn observe(
    target: &WindowIdentity,
    nonce: &str,
    read_content: bool,
) -> Result<Vec<SoftwareControl>, SoftwareError> {
    let budget = Budget::new();
    let _apartment = Apartment::enter()?;
    let automation = client(&budget)?;
    unsafe {
        let request = cache_request(&automation, &budget)?;
        budget.check("observe_root_element")?;
        let root = automation
            .ElementFromHandle(identity::verify(target)?)
            .map_err(|e| error("uia_root_unavailable", e))?;
        budget.check("observe_tree_walker")?;
        let walker = automation
            .ControlViewWalker()
            .map_err(|e| error("uia_unavailable", e))?;
        enum Work {
            Visit(IUIAutomationElement, Vec<u32>, bool),
            Sibling(IUIAutomationElement, Vec<u32>),
        }
        let mut stack = vec![Work::Visit(root, Vec::new(), false)];
        let mut controls = Vec::new();
        let traversal = (|| -> Result<(), SoftwareError> {
            let mut visited = 0;
            while let Some(work) = stack.pop() {
                budget.check("observe_next_node")?;
                if controls.len() >= 64 || visited >= 128 {
                    break;
                }
                match work {
                    Work::Sibling(element, mut path) => {
                        let index = path.last_mut().ok_or_else(|| {
                            error("uia_invalid_path", "Root siblings are outside target scope")
                        })?;
                        if *index >= 63 {
                            continue;
                        }
                        budget.check("observe_next_sibling")?;
                        if let Some(next) = optional_child(walker.GetNextSiblingElement(&element))?
                        {
                            *index += 1;
                            stack.push(Work::Visit(next, path, true));
                        }
                    }
                    Work::Visit(element, path, siblings) => {
                        visited += 1;
                        if siblings {
                            stack.push(Work::Sibling(element.clone(), path.clone()));
                        }
                        let mut descriptor =
                            match describe(&element, &path, nonce, target.pid, &budget, &request) {
                                Ok(c) => c,
                                Err(e)
                                    if matches!(
                                        e.code.as_str(),
                                        "sensitive_control" | "foreign_control"
                                    ) =>
                                {
                                    continue
                                }
                                Err(e) => return Err(e),
                            };
                        if read_content
                            && !element
                                .CurrentIsOffscreen()
                                .map_err(|e| error("uia_provider_failed", e))?
                                .as_bool()
                        {
                            descriptor.visible_text = read_visible_text(&element, &descriptor)?;
                        }
                        let descend = path.len() < 16
                            && (path.is_empty()
                                || !matches!(descriptor.role.as_str(), "edit" | "text" | "window"));
                        controls.push(descriptor);
                        if descend {
                            budget.check("observe_first_child")?;
                            if let Some(child) =
                                optional_child(walker.GetFirstChildElement(&element))?
                            {
                                let mut child_path = path;
                                child_path.push(0);
                                stack.push(Work::Visit(child, child_path, true));
                            }
                        }
                    }
                }
            }
            Ok(())
        })();
        match traversal {
            Err(e) if e.code == "uia_timeout" && !controls.is_empty() => {
                stage("observe_partial_own_budget")
            }
            Err(e) => return Err(e),
            Ok(()) => {}
        }
        identity::verify(target)?;
        stage("observe_complete");
        Ok(controls)
    }
}
pub(super) fn control(
    target: &WindowIdentity,
    expected: &SoftwareControl,
    action: &str,
    value: Option<&str>,
    cancellation: &AtomicBool,
    host_closing: &AtomicBool,
    before_dispatch: &dyn Fn() -> Result<(), SoftwareError>,
) -> Result<(), SoftwareError> {
    let budget = Budget::new();
    let check_cancelled = || {
        if cancellation.load(Ordering::Acquire) || host_closing.load(Ordering::SeqCst) {
            Err(error(
                "cancelled",
                "Software control was stopped or its permission revoked",
            ))
        } else {
            Ok(())
        }
    };
    check_cancelled()?;
    if target.class_name == "#32770" {
        return Err(error(
            "sensitive_dialog",
            "Native dialog controls require direct human interaction",
        ));
    }
    let _apartment = Apartment::enter()?;
    let automation = client(&budget)?;
    unsafe {
        budget.check("control_root_element")?;
        let request = cache_request(&automation, &budget)?;
        let mut element = automation
            .ElementFromHandle(identity::verify(target)?)
            .map_err(|e| error("uia_root_unavailable", e))?;
        budget.check("control_tree_walker")?;
        let walker = automation
            .ControlViewWalker()
            .map_err(|e| error("uia_unavailable", e))?;
        if expected.path.len() > 16 {
            return Err(error(
                "stale_observation",
                "Control path is outside observation bounds",
            ));
        }
        for index in &expected.path {
            budget.check("control_path_step")?;
            if *index >= 64 {
                return Err(error(
                    "stale_observation",
                    "Control path is outside observation bounds",
                ));
            }
            element = walker
                .GetFirstChildElement(&element)
                .map_err(|_| error("stale_observation", "Observed control no longer exists"))?;
            for _ in 0..*index {
                budget.check("control_path_sibling")?;
                element = walker
                    .GetNextSiblingElement(&element)
                    .map_err(|_| error("stale_observation", "Observed control no longer exists"))?;
            }
        }
        let current = describe(
            &element,
            &expected.path,
            "recheck",
            target.pid,
            &budget,
            &request,
        )?;
        if !current.enabled || current.fingerprint != expected.fingerprint {
            return Err(error(
                "stale_observation",
                "Control identity, content or availability changed; observe again",
            ));
        }
        identity::verify(target)?;
        let revalidate = || -> Result<(), SoftwareError> {
            check_cancelled()?;
            budget.check("control_final_recheck")?;
            identity::verify(target)?;
            let latest = describe(
                &element,
                &expected.path,
                "dispatch",
                target.pid,
                &budget,
                &request,
            )?;
            if !latest.enabled || latest.is_password || latest.fingerprint != expected.fingerprint {
                return Err(error(
                    "stale_observation",
                    "Control changed during pattern resolution; observe again",
                ));
            }
            check_cancelled()
        };
        let check_window = || {
            check_cancelled()?;
            identity::verify(target).map(|_| ())
        };
        match action {
            "focus" | "focus_control" => {
                before_dispatch()?;
                revalidate()?;
                element.SetFocus().map_err(|e| {
                    SoftwareError::unknown(format!("UIA focus could not be verified: {e}"))
                })?;
                if !element
                    .CurrentHasKeyboardFocus()
                    .map_err(|e| SoftwareError::unknown(e.to_string()))?
                    .as_bool()
                {
                    return Err(SoftwareError::unknown(
                        "UIA returned but control focus was not observed",
                    ));
                }
                Ok(())
            }
            "select" | "check" | "uncheck" | "expand" | "collapse" if current.supports(action) => {
                set_pattern_state(
                    &element,
                    action,
                    &|| {
                        before_dispatch()?;
                        revalidate()
                    },
                    &check_window,
                )
            }
            "invoke" | "invoke_then_observe" if current.can_invoke => {
                budget.check("control_invoke_pattern")?;
                let pattern = element
                    .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                    .map_err(|e| error("pattern_unavailable", e))?;
                before_dispatch()?;
                revalidate()?;
                pattern.Invoke().map_err(|e| {
                    SoftwareError::unknown(format!("Invocation result uncertain: {e}"))
                })?;
                if action == "invoke_then_observe" {
                    return Ok(());
                }
                // A UIA Invoke acknowledgement cannot prove the application's business result.
                Err(SoftwareError::unknown("UIA invocation was dispatched; verify the application outcome before another action. Do not replay this invocation."))
            }
            "set_value" if current.can_set_value => {
                budget.check("control_value_pattern")?;
                let value = value.ok_or_else(|| error("value_required", "A value is required"))?;
                if value.len() > 16384 || value.contains('\0') {
                    return Err(error("invalid_value", "Value is outside text input bounds"));
                }
                let pattern = element
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .map_err(|e| error("pattern_unavailable", e))?;
                before_dispatch()?;
                revalidate()?;
                pattern.SetValue(&BSTR::from(value)).map_err(|e| {
                    SoftwareError::unknown(format!("SetValue result uncertain: {e}"))
                })?;
                // Compare locally with the approved value, never return existing editor contents.
                verify_readback(
                    || {
                        if element
                            .CurrentIsPassword()
                            .map_err(|e| SoftwareError::unknown(e.to_string()))?
                            .as_bool()
                        {
                            return Err(SoftwareError::unknown("控件已变为密码框，无法核对写入"));
                        }
                        let actual = pattern
                            .CurrentValue()
                            .map_err(|e| SoftwareError::unknown(e.to_string()))?;
                        Ok(actual.iter().copied().eq(value.encode_utf16()))
                    },
                    &check_window,
                )
            }
            _ => Err(error(
                "unsupported_action",
                "This control does not support the requested safe UIA pattern",
            )),
        }
    }
}
