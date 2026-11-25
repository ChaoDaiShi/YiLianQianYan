use super::{client, identity, optional_child, optional_pattern, Apartment, Budget};
use windows::{
    core::BSTR,
    Win32::{
        System::Variant::VARIANT,
        UI::{Accessibility::*, WindowsAndMessaging::IsIconic},
    },
};
use yilian_backend::software::{SoftwareError, WechatConversation, WindowIdentity};

fn unavailable() -> SoftwareError {
    SoftwareError::new(
        "wechat_conversation_unavailable",
        "微信未提供当前会话的无障碍数据，请确认已打开会话",
    )
}

/// Query only the active chat pane. WeChat's conversation list and search results
/// are siblings of ChatDetailView and can contain previews from other chats.
pub(crate) fn read(target: &WindowIdentity) -> Result<WechatConversation, SoftwareError> {
    let hwnd = identity::verify(target)?;
    let budget = Budget::new();
    let _apartment = Apartment::enter()?;
    let client = client(&budget)?;
    let conversation = unsafe {
        let root = client.ElementFromHandle(hwnd).map_err(|_| unavailable())?;
        let find = |root: &IUIAutomationElement, name: &str| {
            budget.check("wechat_conversation_find")?;
            let condition = client
                .CreatePropertyCondition(UIA_ClassNamePropertyId, &VARIANT::from(BSTR::from(name)))
                .map_err(|_| unavailable())?;
            optional_child(root.FindFirst(TreeScope_Descendants, &condition))?
                .ok_or_else(unavailable)
                .and_then(|element| {
                    if element.CurrentProcessId().ok() == Some(target.pid as i32) {
                        Ok(element)
                    } else {
                        Err(unavailable())
                    }
                })
        };
        let pane = find(&root, "mmui::ChatDetailView")?;
        let editor = find(&pane, "mmui::ChatInputField")?;
        let full_title = editor.CurrentName().map_err(|_| unavailable())?.to_string();
        let title: String = full_title.trim().chars().take(160).collect();
        if title.is_empty()
            || editor
                .CurrentIsPassword()
                .map_err(|_| unavailable())?
                .as_bool()
        {
            return Err(unavailable());
        }
        budget.check("wechat_conversation_draft")?;
        let draft = optional_pattern(
            editor.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId),
        )?
        .ok_or_else(unavailable)?
        .CurrentValue()
        .map_err(|_| unavailable())?
        .to_string()
        .chars()
        .take(1024)
        .collect();

        let view = find(&pane, "mmui::MessageView")?;
        let list = find(&view, "mmui::RecyclerListView")?;
        let cache = client.CreateCacheRequest().map_err(|_| unavailable())?;
        for property in [
            UIA_ProcessIdPropertyId,
            UIA_ControlTypePropertyId,
            UIA_IsPasswordPropertyId,
            UIA_NamePropertyId,
            UIA_ClassNamePropertyId,
        ] {
            cache.AddProperty(property).map_err(|_| unavailable())?;
        }
        let condition = client.CreateTrueCondition().map_err(|_| unavailable())?;
        budget.check("wechat_conversation_messages")?;
        let rows = list
            .FindAllBuildCache(TreeScope_Children, &condition, &cache)
            .map_err(|_| unavailable())?;
        let count = rows.Length().map_err(|_| unavailable())?;
        let mut messages = Vec::new();
        let mut message_rows = Vec::new();
        for index in (count - 24).max(0)..count {
            budget.check("wechat_conversation_row")?;
            let row = rows.GetElement(index).map_err(|_| unavailable())?;
            if row.CachedProcessId().ok() != Some(target.pid as i32)
                || row.CachedControlType().ok() != Some(UIA_ListItemControlTypeId)
                || row.CachedIsPassword().map_or(true, |value| value.as_bool())
            {
                continue;
            }
            let message: String = row
                .CachedName()
                .map_err(|_| unavailable())?
                .to_string()
                .trim()
                .chars()
                .take(320)
                .collect();
            if !message.is_empty() {
                // The plain ChatItemView also represents date and time separators.
                if row
                    .CachedClassName()
                    .map_err(|_| unavailable())?
                    .to_string()
                    != "mmui::ChatItemView"
                {
                    message_rows.push(message.clone());
                }
                messages.push(message);
            }
        }
        if find(&pane, "mmui::ChatInputField")?
            .CurrentName()
            .map_err(|_| unavailable())?
            .to_string()
            != full_title
        {
            return Err(unavailable());
        }
        WechatConversation {
            title,
            messages,
            message_rows,
            draft,
            minimized: IsIconic(hwnd).as_bool(),
        }
    };
    identity::verify(target)?;
    Ok(conversation)
}
