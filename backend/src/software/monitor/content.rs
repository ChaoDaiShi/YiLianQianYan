use super::super::{Registration, SoftwareControl, SoftwareHost, SoftwareRuntime, WindowIdentity};
use serde::Serialize;
use std::collections::BTreeSet;

pub(super) struct Snapshot {
    revision: u64,
    source: Source,
    lines: BTreeSet<String>,
    chat: Option<(String, Vec<String>)>,
}

#[derive(Debug, PartialEq, Eq)]
enum Source {
    Controls,
    WindowText,
    WechatConversation,
}

#[derive(Serialize)]
pub(super) struct Delta {
    #[serde(skip)]
    pub(super) revision: u64,
    removed: Vec<String>,
    added: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wechat_chat: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) new_messages: Vec<String>,
}

pub(super) fn authorized(
    runtime: &SoftwareRuntime,
    registration: &Registration,
    session_id: &str,
    target: &WindowIdentity,
) -> bool {
    runtime.registration(&registration.id).is_ok_and(|current| {
        current.revision == registration.revision && current.config.allow_observe
    }) && runtime
        .state
        .lock()
        .sessions
        .get(session_id)
        .is_some_and(|session| {
            session.registration_id == registration.id
                && session.state == "active"
                && session.window.identity == *target
        })
}

pub(super) fn sample(
    runtime: &SoftwareRuntime,
    host: &dyn SoftwareHost,
    registration: &Registration,
    session_id: &str,
    target: &WindowIdentity,
) -> Option<Snapshot> {
    if !authorized(runtime, registration, session_id, target) {
        return None;
    }
    let conversation = (registration.config.adapter == "wechat"
        && registration.config.allow_content)
        .then(|| host.wechat_conversation(target).ok())
        .flatten();
    let (source, lines, chat) = if let Some(conversation) = conversation {
        let mut lines = BTreeSet::new();
        if !conversation.draft.is_empty() {
            collect_text(&mut lines, &format!("未发送草稿：{}", conversation.draft));
        }
        (
            Source::WechatConversation,
            lines,
            Some((conversation.title, conversation.message_rows)),
        )
    } else {
        // Monitoring leaves the chat's actionable observation untouched.
        let controls = host
            .observe(
                target,
                "software-monitor",
                registration.config.allow_content,
            )
            .unwrap_or_default();
        let (source, lines) = sample_lines(
            &controls,
            registration.config.allow_content,
            registration.config.allow_window_ocr,
            || {
                runtime.check_window_pixels().ok()?;
                let text = host.recognize_window(target).ok()?;
                runtime.check_window_pixels().ok()?;
                Some(text)
            },
        );
        (source, lines, None)
    };
    if !authorized(runtime, registration, session_id, target) {
        return None;
    }
    (chat.is_some() || !lines.is_empty()).then_some(Snapshot {
        revision: registration.revision,
        source,
        lines,
        chat,
    })
}

fn collect_text(lines: &mut BTreeSet<String>, text: &str) {
    for line in text.lines() {
        let text: String = line.trim().chars().take(160).collect();
        if !text.is_empty() {
            lines.insert(text);
        }
    }
}

fn sample_lines(
    controls: &[SoftwareControl],
    read_content: bool,
    read_window_text: bool,
    read_pixels: impl FnOnce() -> Option<String>,
) -> (Source, BTreeSet<String>) {
    let mut lines = collect(controls, read_content);
    // A window shell or pane can expose state without exposing any application text.
    let has_body = controls.iter().any(|control| {
        !control.is_password
            && !matches!(control.role.as_str(), "window" | "pane")
            && control
                .visible_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
    });
    if read_content && read_window_text && !has_body {
        let mut window_lines = BTreeSet::new();
        if let Some(text) = read_pixels() {
            collect_text(&mut window_lines, &text);
        }
        if !window_lines.is_empty() {
            lines.extend(window_lines);
            return (Source::WindowText, lines);
        }
    }
    (Source::Controls, lines)
}

fn collect(controls: &[SoftwareControl], read_content: bool) -> BTreeSet<String> {
    let mut lines = BTreeSet::new();
    for control in controls.iter().filter(|control| !control.is_password) {
        if read_content {
            if let Some(text) = &control.visible_text {
                collect_text(&mut lines, text);
            }
        }
        let Some(state) = control
            .toggle_state
            .as_deref()
            .or_else(|| {
                control
                    .selected
                    .map(|selected| if selected { "selected" } else { "unselected" })
            })
            .or(control.expand_state.as_deref())
        else {
            continue;
        };
        let label = if !control.name.is_empty() {
            &control.name
        } else if !control.automation_id.is_empty() {
            &control.automation_id
        } else {
            &control.role
        };
        lines.insert(format!("{label} [{state}]"));
    }
    lines
}

pub(super) fn change(previous: &mut Option<Snapshot>, current: Option<Snapshot>) -> Option<Delta> {
    let last = std::mem::replace(previous, current);
    let (last, current) = (last?, previous.as_ref()?);
    if last.revision != current.revision || last.source != current.source {
        return None;
    }
    let (wechat_chat, new_messages) = match (&last.chat, &current.chat) {
        (Some((before_title, before)), Some((title, after))) => {
            if before_title != title {
                return None;
            }
            (Some(title.clone()), appended_messages(before, after))
        }
        _ => (None, Vec::new()),
    };
    if last.lines == current.lines && new_messages.is_empty() {
        return None;
    }
    Some(Delta {
        revision: current.revision,
        removed: last
            .lines
            .difference(&current.lines)
            .take(6)
            .cloned()
            .collect(),
        added: current
            .lines
            .difference(&last.lines)
            .take(6)
            .cloned()
            .collect(),
        wechat_chat,
        new_messages,
    })
}

fn appended_messages(before: &[String], after: &[String]) -> Vec<String> {
    // Match the newest known rows against the beginning of the current view.
    // A different chat or a jump to older history starts a new baseline.
    let overlap = (1..=before.len().min(after.len()))
        .rev()
        .find(|&count| before[before.len() - count..] == after[..count]);
    let Some(overlap) = overlap.or_else(|| before.is_empty().then_some(0)) else {
        return Vec::new();
    };
    after[overlap..].iter().take(6).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_wechat_message_appended_still_counts_as_a_new_message() {
        let snapshot = |title: &str, messages: &[&str]| Snapshot {
            revision: 1,
            source: Source::WechatConversation,
            lines: BTreeSet::new(),
            chat: Some((
                title.to_owned(),
                messages
                    .iter()
                    .map(|message| (*message).to_owned())
                    .collect(),
            )),
        };
        let mut previous = None;
        assert!(change(&mut previous, Some(snapshot("原会话", &["早", "收到"]))).is_none());
        assert_eq!(
            change(
                &mut previous,
                Some(snapshot("原会话", &["早", "收到", "收到"]))
            )
            .unwrap()
            .new_messages,
            ["收到"],
        );
        assert!(change(
            &mut previous,
            Some(snapshot("原会话", &["早", "收到", "收到"]))
        )
        .is_none());
        assert!(change(&mut previous, Some(snapshot("另一会话", &["收到", "好"]))).is_none());
        assert_eq!(
            change(&mut previous, Some(snapshot("另一会话", &["好", "好"])))
                .unwrap()
                .new_messages,
            ["好"],
        );
        assert!(appended_messages(
            &["第二条".into(), "第三条".into()],
            &["第一条".into(), "第二条".into()]
        )
        .is_empty());
        assert!(appended_messages(&["旧会话末尾".into()], &["历史消息".into()]).is_empty());
    }

    #[test]
    fn window_shell_without_body_uses_granted_ocr() {
        let mut reads = 0;
        let mut read_pixels = || {
            reads += 1;
            Some("文件传输助手\n\n  YILIAN 472  ".to_string())
        };
        let (source, lines) = sample_lines(&[], true, true, &mut read_pixels);
        assert_eq!(source, Source::WindowText);
        assert_eq!(
            lines,
            BTreeSet::from(["文件传输助手".into(), "YILIAN 472".into()])
        );

        let (source, lines) = sample_lines(&[], true, false, &mut read_pixels);
        assert_eq!(source, Source::Controls);
        assert!(lines.is_empty());
        let (_, lines) = sample_lines(&[], false, true, &mut read_pixels);
        assert!(lines.is_empty());
        assert_eq!(reads, 1);
    }

    #[test]
    fn first_read_and_identical_snapshots_do_not_emit_changes() {
        let snapshot = |revision, text: &str| Snapshot {
            revision,
            source: Source::Controls,
            lines: BTreeSet::from([text.to_owned()]),
            chat: None,
        };
        let mut previous = None;
        assert!(change(&mut previous, Some(snapshot(1, "待审核"))).is_none());
        assert!(change(&mut previous, Some(snapshot(1, "待审核"))).is_none());
        let delta = change(&mut previous, Some(snapshot(1, "审核完成"))).unwrap();
        assert_eq!(delta.removed, ["待审核"]);
        assert_eq!(delta.added, ["审核完成"]);
        assert!(change(&mut previous, Some(snapshot(2, "已发货"))).is_none());
        assert!(change(&mut previous, None).is_none());
        assert!(change(&mut previous, Some(snapshot(2, "已送达"))).is_none());
        assert!(change(
            &mut previous,
            Some(Snapshot {
                source: Source::WindowText,
                ..snapshot(2, "已签收")
            })
        )
        .is_none());
    }
}
