use super::*;

#[allow(non_upper_case_globals)]
pub(super) unsafe fn describe_patterns(
    element: &IUIAutomationElement,
    role: UIA_CONTROLTYPE_ID,
) -> Result<(Option<bool>, Option<String>, Option<String>), SoftwareError> {
    let mut selected = None;
    let mut toggle = None;
    let mut expanded = None;
    if matches!(
        role,
        UIA_ListItemControlTypeId
            | UIA_TabItemControlTypeId
            | UIA_RadioButtonControlTypeId
            | UIA_TreeItemControlTypeId
            | UIA_DataItemControlTypeId
            | UIA_MenuItemControlTypeId
    ) {
        if let Some(pattern) = optional_pattern(
            element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            ),
        )? {
            selected = Some(
                pattern
                    .CurrentIsSelected()
                    .map_err(|e| error("uia_provider_failed", e))?
                    .as_bool(),
            );
        }
    }
    if matches!(
        role,
        UIA_ButtonControlTypeId | UIA_CheckBoxControlTypeId | UIA_MenuItemControlTypeId
    ) {
        if let Some(pattern) = optional_pattern(
            element.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId),
        )? {
            toggle = Some(
                match pattern
                    .CurrentToggleState()
                    .map_err(|e| error("uia_provider_failed", e))?
                {
                    ToggleState_On => "on",
                    ToggleState_Off => "off",
                    _ => "mixed",
                }
                .into(),
            );
        }
    }
    if matches!(
        role,
        UIA_ComboBoxControlTypeId
            | UIA_TreeItemControlTypeId
            | UIA_MenuItemControlTypeId
            | UIA_ButtonControlTypeId
    ) {
        if let Some(pattern) = optional_pattern(
            element.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                UIA_ExpandCollapsePatternId,
            ),
        )? {
            expanded = Some(
                match pattern
                    .CurrentExpandCollapseState()
                    .map_err(|e| error("uia_provider_failed", e))?
                {
                    ExpandCollapseState_Expanded => "expanded",
                    ExpandCollapseState_Collapsed => "collapsed",
                    ExpandCollapseState_LeafNode => "leaf",
                    _ => "partial",
                }
                .into(),
            );
        }
    }
    Ok((selected, toggle, expanded))
}

pub(super) unsafe fn read_visible_text(
    element: &IUIAutomationElement,
    control: &SoftwareControl,
) -> Result<Option<String>, SoftwareError> {
    if matches!(control.role.as_str(), "edit" | "document") {
        if let Some(pattern) = optional_pattern(
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId),
        )? {
            if let Ok(ranges) = pattern.GetVisibleRanges() {
                let mut lines = Vec::new();
                for index in 0..ranges.Length().unwrap_or(0).min(4) {
                    if let Ok(text) = ranges
                        .GetElement(index)
                        .and_then(|range| range.GetText(256))
                    {
                        lines.push(text.to_string());
                    }
                }
                let text = lines.join("\n");
                if !text.is_empty() {
                    return Ok(Some(text));
                }
            }
        }
    }
    if control.name.is_empty()
        && matches!(
            control.role.as_str(),
            "window"
                | "edit"
                | "document"
                | "text"
                | "list_item"
                | "tree_item"
                | "data_item"
                | "link"
                | "group"
        )
    {
        let label: String = element
            .CurrentName()
            .map_err(|e| error("uia_provider_failed", e))?
            .to_string()
            .chars()
            .take(240)
            .collect();
        return Ok((!label.is_empty()).then_some(label));
    }
    Ok(None)
}

// Chromium and some other providers publish new accessibility state after the action returns.
// Poll only the readback. Dispatching a toggle twice can undo the requested change.
pub(super) fn verify_readback(
    mut read: impl FnMut() -> Result<bool, SoftwareError>,
    check_window: &dyn Fn() -> Result<(), SoftwareError>,
) -> Result<(), SoftwareError> {
    let deadline = Instant::now() + Duration::from_millis(800);
    loop {
        check_window().map_err(|e| SoftwareError::unknown(e.to_string()))?;
        if read()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(SoftwareError::unknown("已操作，未能在原窗口核对到目标状态"));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

pub(super) unsafe fn set_pattern_state(
    element: &IUIAutomationElement,
    action: &str,
    before_dispatch: &dyn Fn() -> Result<(), SoftwareError>,
    check_window: &dyn Fn() -> Result<(), SoftwareError>,
) -> Result<(), SoftwareError> {
    let unavailable = |e| error("pattern_unavailable", e);
    let uncertain = |e: windows::core::Error| SoftwareError::unknown(e.to_string());
    match action {
        "select" => {
            let p = element
                .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                    UIA_SelectionItemPatternId,
                )
                .map_err(unavailable)?;
            before_dispatch()?;
            if !p.CurrentIsSelected().map_err(unavailable)?.as_bool() {
                p.Select().map_err(uncertain)?;
            }
            verify_readback(
                || Ok(p.CurrentIsSelected().map_err(uncertain)?.as_bool()),
                check_window,
            )
        }
        "check" | "uncheck" => {
            let p = element
                .GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId)
                .map_err(unavailable)?;
            let desired = if action == "check" {
                ToggleState_On
            } else {
                ToggleState_Off
            };
            before_dispatch()?;
            if p.CurrentToggleState().map_err(unavailable)? != desired {
                p.Toggle().map_err(uncertain)?;
            }
            verify_readback(
                || Ok(p.CurrentToggleState().map_err(uncertain)? == desired),
                check_window,
            )
        }
        "expand" | "collapse" => {
            let p = element
                .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                    UIA_ExpandCollapsePatternId,
                )
                .map_err(unavailable)?;
            let desired = if action == "expand" {
                ExpandCollapseState_Expanded
            } else {
                ExpandCollapseState_Collapsed
            };
            before_dispatch()?;
            if p.CurrentExpandCollapseState().map_err(unavailable)? != desired {
                if action == "expand" {
                    p.Expand()
                } else {
                    p.Collapse()
                }
                .map_err(uncertain)?;
            }
            verify_readback(
                || Ok(p.CurrentExpandCollapseState().map_err(uncertain)? == desired),
                check_window,
            )
        }
        _ => Err(error("unsupported_action", action)),
    }
}
