use super::*;

#[test]
fn relation_proposals_require_verbatim_user_evidence_and_starts_as_inferred() {
    let text = "星桥发布项目依赖接口验收。地图项目和小林仅在同一段出现。";
    let sources = [("message".into(), text.into())].into_iter().collect();
    let result = relationship_drafts(
        "conversation",
        &sources,
        serde_json::from_value(json!({"relationships":[
            {"message_id":"message","evidence":"星桥发布项目依赖接口验收","kind":"project",
            "subject":"星桥发布项目","label":"依赖","object":"接口验收","directed":true,"certainty":"confirmed"},
            {"message_id":"message","evidence":"小林负责地图项目","kind":"project",
            "subject":"地图项目","label":"负责人","object":"小林","directed":true},
            {"message_id":"assistant","evidence":"星桥发布项目依赖接口验收","kind":"project",
            "subject":"星桥发布项目","label":"依赖","object":"接口验收","directed":true}
        ]})).unwrap(),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content, "星桥发布项目 · 依赖 · 接口验收 · 待确认");
}

#[test]
fn saved_text_extraction_anchors_unicode_and_rejects_fabricated_sources() {
    let text = "🪴提醒：设计组偏好使用深色主题。";
    let result = saved_text_relationship_drafts("memory-a", text,
        serde_json::from_value(json!({"relationships":[
            {"message_id":"memory-a","evidence":"设计组偏好使用深色主题","kind":"preference",
             "subject":"设计组","label":"偏好","object":"深色主题","directed":true},
            {"message_id":"memory-a","evidence":"设计组偏好使用深色主题","kind":"preference",
             "subject":"设计组","label":"偏好","object":"深色主题","directed":true,"occurredAt":"2026-01-01"},
            {"message_id":"neighbor","evidence":"设计组偏好使用深色主题","kind":"preference",
             "subject":"设计组","label":"偏好","object":"深色主题","directed":true},
            {"message_id":"memory-a","evidence":"设计组偏好使用深色主题","kind":"preference",
             "subject":"不存在的主体","label":"偏好","object":"深色主题","directed":true}
        ]})).unwrap());
    assert_eq!(result.len(), 1);
    let (_, start, end) = &result[0];
    assert_eq!(*start, 4);
    assert_eq!(
        text.chars()
            .skip(*start)
            .take(end - start)
            .collect::<String>(),
        "设计组偏好使用深色主题"
    );
}
