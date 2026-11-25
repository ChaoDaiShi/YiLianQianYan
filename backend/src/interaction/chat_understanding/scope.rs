use super::{has, Scope};

pub(crate) fn allows_read_tool(name: &str) -> bool {
    matches!(
        name,
        "ask_user"
            | "read_file"
            | "grep"
            | "glob"
            | "ai_search"
            | "vision_analysis"
            | "write_todos"
            | "visible_browser_tabs"
            | "visible_browser_observe"
    )
}

/// A prohibition on delivery can accompany an explicit request to edit the local composer.
pub(super) fn requests_wechat_draft(text: &str, previous: Option<&str>) -> bool {
    let wechat = |s: &str| s.contains("微信") || s.to_ascii_lowercase().contains("wechat");
    let refers_to_editor = has(
        text,
        &[
            "对应的输入框",
            "刚才的输入框",
            "这个群",
            "那个群",
            "刚才的群",
        ],
    );
    let edit_followup = [
        "改成",
        "换成",
        "补成",
        "再加个",
        "把它改成",
        "把刚才的草稿",
        "把草稿",
        "草稿改成",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
        && previous.is_some_and(|s| wechat(s) && requests_wechat_draft(s, None));
    if !(wechat(text) || refers_to_editor && previous.is_some_and(wechat) || edit_followup) {
        return false;
    }
    let mut request = text.to_owned();
    for restriction in [
        "先不要发送",
        "先不要发",
        "不要发送",
        "不要发出去",
        "不要发",
        "先别发送",
        "先别发",
        "别发送",
        "别发出去",
        "别发",
        "先不发送",
        "先不发",
        "不用发送",
        "不发送",
        "不发",
    ] {
        request = request.replace(restriction, "");
    }
    !explicit_read_only(&request)
        && !has(
            &request,
            &[
                "怎么",
                "如何",
                "为什么",
                "是不是",
                "不要",
                "不用",
                "别",
                "发送",
                "发给",
                "发出去",
                "转发",
                "教我",
                "输入法",
                "不需要",
                "无需",
                "不想",
                "只读",
                "只看",
            ],
        )
        && (edit_followup
            || has(&request, &["输入", "键入", "打个", "打上", "填入", "填上"])
            || request.contains("群") && has(&request, &["写个", "写上", "写下"])
            || request.contains("草稿")
                && has(
                    &request,
                    &["写", "改成", "补成", "填", "追加", "加上", "加个", "换成"],
                ))
}

pub(crate) fn explicit_read_only(text: &str) -> bool {
    has(
        &text.to_lowercase(),
        &[
            "先别",
            "先不",
            "不要执行",
            "不用执行",
            "不得",
            "禁止",
            "不要改",
            "不要写",
            "不要发送",
            "别发",
            "不要删除",
            "只预览",
            "别改",
            "不要动",
            "不用改",
            "不修改",
            "只读",
            "仅讨论",
            "只讨论",
            "只给方案",
            "仅给方案",
            "don't change",
            "do not execute",
            "read-only",
        ],
    )
}

pub(super) fn mounted_scope(
    text: &str,
    previous_user: Option<&str>,
    software_names: &[String],
) -> Option<Scope> {
    let mentions_window = |s: &str| {
        has(
            s,
            &[
                "挂载窗口",
                "挂载的",
                "已连接的软件",
                "当前软件",
                "这个窗口",
                "当前窗口",
                "当前应用",
                "mounted window",
            ],
        ) || software_names
            .iter()
            .any(|name| !name.is_empty() && s.contains(&name.to_lowercase()))
    };
    let refers_to_software = mentions_window(text)
        || (has(
            text,
            &[
                "刚才",
                "继续",
                "再把",
                "改成",
                "换成",
                "再点",
                "再选",
                "再输入",
                "再看看",
                "里面",
                "这个输入框",
            ],
        ) && previous_user.is_some_and(|s| mentions_window(&s.to_lowercase())));
    let discussion = has(
        text,
        &[
            "怎么",
            "如何",
            "教程",
            "教我",
            "为什么",
            "设计方案",
            "源码",
            "源代码",
            "帮我开发",
        ],
    );
    let command_words = text
        .replace("输入框", "控件")
        .replace("复选框", "控件")
        .replace("选择框", "控件");
    // A state question can contain the same verb as a command (e.g. “是否勾选”).
    let mut command_words = command_words;
    let mut state_question = false;
    for verb in ["勾选", "选中", "展开", "折叠"] {
        for question in [
            format!("是否{verb}"),
            format!("有没有{verb}"),
            format!("{verb}了吗"),
        ] {
            if command_words.contains(&question) {
                command_words = command_words.replace(&question, "");
                state_question = true;
            }
        }
    }
    let ui_write = has(
        &command_words,
        &[
            "输入",
            "填写",
            "填入",
            "键入",
            "写入",
            "改成",
            "换成",
            "写上",
            "写下",
            "设为",
            "点击",
            "点一下",
            "按下",
            "选择",
            "选中",
            "勾选",
            "取消勾选",
            "展开",
            "折叠",
            "收起",
            "切换",
            "聚焦",
            "click ",
            "type ",
            "select ",
        ],
    );
    let ui_read = state_question
        || has(
            text,
            &[
                "看",
                "读",
                "显示",
                "界面",
                "内容",
                "文字",
                "什么",
                "列出",
                "有哪些",
                "状态",
                "总结",
                "概括",
                "read ",
                "show ",
            ],
        );
    if refers_to_software && !discussion && (ui_write || ui_read) {
        // A local edit may carry an independent restriction on saving or sending.
        let mut edit_request = text.to_owned();
        for suffix in [
            "先别保存",
            "不要保存",
            "先别发送",
            "先别发",
            "不要发送",
            "先不提交",
            "不要提交",
        ] {
            edit_request = edit_request.replace(suffix, "");
        }
        Some(
            if ui_write
                && !explicit_read_only(&edit_request)
                && !has(
                    &edit_request,
                    &[
                        "不要输入",
                        "别输入",
                        "不要点击",
                        "不要勾选",
                        "只看",
                        "只需看",
                    ],
                )
            {
                Scope::SoftwareControl
            } else {
                Scope::SoftwareRead
            },
        )
    } else {
        None
    }
}
