use super::*;

#[test]
fn evaluates_distinct_conversation_scenarios() {
    let scenarios = [
        ("早安！", None, Exchange::Chat, Scope::Conversation),
        ("我今天很难过", None, Exchange::Chat, Scope::Conversation),
        (
            "帮我查一下本地资料里的研究结论",
            None,
            Exchange::Work,
            Scope::Read,
        ),
        (
            "我很紧张，请把测试报告保存为报告.md",
            None,
            Exchange::Mixed,
            Scope::Work,
        ),
        ("请把它删除", None, Exchange::Work, Scope::Conversation),
        ("请删除文件，先不要删除", None, Exchange::Work, Scope::Read),
        (
            "请把刚才的结论写入报告.md",
            Some("比较测试报告的结果"),
            Exchange::Work,
            Scope::Work,
        ),
        ("如此这般", None, Exchange::Uncertain, Scope::Conversation),
        (
            "write the isolated regression output",
            None,
            Exchange::Work,
            Scope::Work,
        ),
    ];
    for (message, context, exchange, scope) in scenarios {
        let actual = understand(message, context, false, false, WorkspaceMode::Auto);
        assert_eq!(
            (actual.exchange, actual.scope),
            (exchange, scope),
            "{message}"
        );
        assert!(!actual.memory_change, "{message}");
    }
}

#[test]
fn semantic_labels_do_not_change_execution_or_memory_policy() {
    let mut actual = understand(
        "先别改代码，帮我聊聊",
        None,
        false,
        false,
        WorkspaceMode::Work,
    );
    actual.apply_semantic(SemanticLabels {
        exchange: Exchange::Work,
        actions: vec![Action::Execute],
        domains: vec![Domain::Memory],
    });
    assert_eq!(actual.scope, Scope::Read);
    assert!(!actual.memory_change);
}

#[test]
fn wechat_draft_instruction_permits_only_draft_work() {
    for text in [
        "帮我在微信的 cs一群 把草稿补成 cs2，先别发。",
        "在微信当前群聊输入 cs，不要发送",
        "微信当前群里打个 cs 先别发",
        "在微信的 cs一群 里写个 cs",
    ] {
        let intent = understand(text, None, false, false, WorkspaceMode::Auto);
        assert_eq!(
            serde_json::to_value(intent.scope).unwrap(),
            "wechat_draft",
            "{text}"
        );
        assert_eq!(intent.role(WorkspaceMode::Auto), Role::General);
    }
    for text in [
        "在微信输入 cs，先别执行",
        "不要在微信输入 cs",
        "微信的群聊里怎么输入文字？",
        "请检查微信草稿，先别发送",
        "帮我开发微信输入法",
        "只读微信，看看群里输入了什么",
    ] {
        assert_ne!(
            serde_json::to_value(understand(text, None, false, false, WorkspaceMode::Auto).scope)
                .unwrap(),
            "wechat_draft",
            "{text}"
        );
    }
}

#[test]
fn follows_an_explicit_reference_to_the_wechat_editor() {
    let request = "尝试在对应的输入框中输入文字，例如 cs";
    assert_eq!(
        understand(
            request,
            Some("看看微信当前群聊"),
            false,
            false,
            WorkspaceMode::Auto
        )
        .scope,
        Scope::WechatDraft
    );
    assert_ne!(
        understand(
            request,
            Some("看看浏览器页面"),
            false,
            false,
            WorkspaceMode::Auto
        )
        .scope,
        Scope::WechatDraft
    );
}

#[test]
fn natural_wechat_draft_followup_reuses_the_previous_editor_only() {
    let previous = Some("帮我在微信的 cs一群 把草稿补成 cs2，先别发。");
    for request in ["改成 cs3，先别发。", "把刚才的草稿换成 cs3", "再加个 2"] {
        assert_eq!(
            understand(request, previous, false, false, WorkspaceMode::Auto).scope,
            Scope::WechatDraft,
            "{request}"
        );
    }
    for request in [
        "把报告改成 PDF",
        "别改草稿",
        "怎么换成 cs3？",
        "改成 cs3 然后发送",
    ] {
        assert_ne!(
            understand(request, previous, false, false, WorkspaceMode::Auto).scope,
            Scope::WechatDraft,
            "{request}"
        );
    }
    assert_ne!(
        understand(
            "改成 cs3",
            Some("微信支付怎么用？"),
            false,
            false,
            WorkspaceMode::Auto
        )
        .scope,
        Scope::WechatDraft
    );
}

#[test]
fn mounted_window_requests_separate_observation_from_action() {
    for (request, scope) in [
        ("看看挂载窗口里有什么", "software_read"),
        ("当前窗口的复选框有没有勾选？", "software_read"),
        ("看看当前窗口输入框里有什么", "software_read"),
        ("把当前窗口输入框改成 cs2，先别保存", "software_control"),
        ("在挂载窗口里勾选自动换行", "software_control"),
        ("只读，看看挂载窗口里的按钮", "software_read"),
    ] {
        let route = understand(request, None, false, false, WorkspaceMode::Auto);
        assert_eq!(
            serde_json::to_value(route.scope).unwrap(),
            scope,
            "{request}"
        );
    }
}
