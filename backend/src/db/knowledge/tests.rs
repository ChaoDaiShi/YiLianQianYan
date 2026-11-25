use super::*;

#[test]
fn curated_entry_resolves_exact_unicode_excerpt_and_retains_removed_source() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let (document, _) = db
        .import_knowledge(
            "resource",
            "原文.txt",
            "text/plain",
            "file",
            20,
            "甲𠮷甲证据乙甲",
        )
        .unwrap();
    let hit = db.search_knowledge("证据").unwrap().remove(0);
    let draft = KnowledgeEntryDraft {
        topic: "项目资料".into(),
        title: "原文结论".into(),
        summary: "用户整理的知识正文".into(),
        sources: vec![KnowledgeSourceAnchor {
            document_id: document.id.clone(),
            version: hit.version,
            start_char: 3,
            end_char: 5,
        }],
    };
    let entry = db.save_knowledge_entry(None, &draft).unwrap();
    assert_eq!(entry.sources[0].excerpt.as_deref(), Some("证据"));
    assert_eq!(entry.sources[0].status, KnowledgeSourceStatus::Available);

    db.conn()
        .execute(
            "UPDATE knowledge_documents SET text_content='甲𠮷甲其他乙甲' WHERE id=?1",
            [&document.id],
        )
        .unwrap();
    let changed = db.knowledge_entry(&entry.entry.id).unwrap().unwrap();
    assert_eq!(changed.sources[0].status, KnowledgeSourceStatus::Changed);
    assert!(changed.sources[0].excerpt.is_none());
    assert!(db.save_knowledge_entry(None, &draft).is_err());

    assert!(db.delete_knowledge_document(&document.id).unwrap());
    let removed = db.knowledge_entry(&entry.entry.id).unwrap().unwrap();
    assert_eq!(removed.sources[0].status, KnowledgeSourceStatus::Removed);
    assert_eq!(removed.sources[0].document_name, "原文.txt");
    assert_eq!(removed.entry.summary, "用户整理的知识正文");
    assert!(removed.sources[0].excerpt.is_none());
    assert!(db.knowledge_documents().unwrap().is_empty());

    let edited = db
        .save_knowledge_entry(
            Some(&entry.entry.id),
            &KnowledgeEntryDraft {
                summary: "更新后的摘要".into(),
                ..draft
            },
        )
        .unwrap();
    assert_eq!(edited.sources[0].status, KnowledgeSourceStatus::Removed);
    assert_eq!(edited.entry.summary, "更新后的摘要");
    assert!(db.delete_knowledge_entry(&entry.entry.id).unwrap());
    assert!(db.knowledge_entry(&entry.entry.id).unwrap().is_none());
}
#[test]
fn knowledge_import_is_complete_searchable_idempotent_and_deletes_only_its_chunks() {
    let path = std::env::temp_dir().join(format!("knowledge-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let text = format!("{}\n全文末尾检索标记", "导入资料内容。".repeat(2600));
    let (document, created) = db
        .import_knowledge(
            "resource-1",
            "资料.md",
            "text/markdown",
            "hash-1",
            text.len() as i64,
            &text,
        )
        .unwrap();
    assert!(created);
    assert!(document.char_count > 12000);
    assert!(document.chunk_count > 10);
    let chunks = db.knowledge_chunks(Some(&document.id), None).unwrap();
    assert_eq!(
        chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<String>(),
        text
    );
    let found = db.search_knowledge("全文末尾检索标记").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].document_id, document.id);
    let (again, created) = db
        .import_knowledge(
            "resource-2",
            "第二个名称.md",
            "text/markdown",
            "hash-1",
            text.len() as i64,
            &text,
        )
        .unwrap();
    assert!(!created);
    assert_eq!(again.id, document.id);
    assert_eq!(
        db.list_memories(&crate::db::MemoryQuery {
            category: Some("knowledge".into()),
            source: Some("document".into()),
            q: None,
            limit: Some(2000),
            offset: None
        })
        .unwrap()
        .len(),
        chunks.len()
    );
    let other = db
        .create_memory(&crate::db::CreateMemoryRequest {
            content: "保留的知识".into(),
            category: "knowledge".into(),
            source: "manual".into(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();
    drop(db);
    let db = Database::new(&path).unwrap();
    assert_eq!(db.knowledge_documents().unwrap().len(), 1);
    assert!(db.delete_knowledge_document(&document.id).unwrap());
    assert!(db.knowledge_documents().unwrap().is_empty());
    assert!(db.knowledge_chunks(None, None).unwrap().is_empty());
    assert!(db.find_memory(&other.id).unwrap().is_some());
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn chunking_respects_unicode_and_keeps_all_text() {
    let text = "𠮷中文\n".repeat(1000);
    let chunks = split_document(&text);
    assert!(chunks.iter().all(|c| c.chars().count() <= 1200));
    assert_eq!(chunks.concat(), text);
    assert!(split_document(" \n\t").is_empty());
}

#[test]
fn chunking_prefers_whole_paragraphs_and_sentences_over_the_fixed_limit() {
    let paragraph = format!("{}\n\n", "𠮷".repeat(870));
    let sentence = format!("{}。", "文".repeat(780));
    let tail = "后".repeat(540);
    let text = format!("{paragraph}{sentence}{tail}");
    assert_eq!(split_document(&text), vec![paragraph, sentence, tail]);
}

#[test]
fn import_keeps_headings_sheets_and_slides_with_their_own_content() {
    let path = std::env::temp_dir().join(format!("knowledge-sections-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let first_sheet = format!("[成本]\n{}\t420\n", "甲".repeat(950));
    let second_sheet = format!("[结余]\n{}\t380\n", "乙".repeat(790));
    for (name, mime, first, second) in [
        (
            "指南.md",
            "text/markdown",
            "# 概述\r\n介绍。\r\n\r\n",
            "## 步骤\r\n步骤说明。\r\n",
        ),
        (
            "汇总.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            first_sheet.as_str(),
            second_sheet.as_str(),
        ),
        (
            "简报.pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "第 1 页\n背景\n",
            "第 2 页\n结论\n备注：\n核对来源\n",
        ),
    ] {
        let text = format!("{first}{second}");
        let (document, _) = db
            .import_knowledge(name, name, mime, name, text.len() as i64, &text)
            .unwrap();
        let chunks = db.knowledge_chunks(Some(&document.id), None).unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.content.as_str())
                .collect::<Vec<_>>(),
            vec![first, second],
            "{name}"
        );
        assert_eq!(
            db.knowledge_text(&document.id).unwrap().as_deref(),
            Some(text.as_str())
        );
    }
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn canonical_search_excerpt_respects_sections_and_reports_its_true_offset() {
    let path = std::env::temp_dir().join(format!(
        "knowledge-structured-search-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = Database::new(&path).unwrap();
    let prior = format!("# 背景\n{}\n\n", "𠮷".repeat(380));
    let section = "## 统计结果\n指标 | 数量\nORCHID_FOCUS | 420\n";
    let text = format!("{prior}{section}## 附录\n与结果无关。");
    db.import_knowledge(
        "r",
        "资料.md",
        "text/markdown",
        "structure",
        text.len() as i64,
        &text,
    )
    .unwrap();
    let hits = db.search_knowledge("ORCHID_FOCUS").unwrap();
    assert_eq!(hits[0].start_char, prior.chars().count());
    assert_eq!(hits[0].content, section);
    let context = db.knowledge_context("ORCHID_FOCUS", 2).unwrap();
    assert_eq!(context[0].content, section);
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn trailing_whitespace_remains_in_canonical_text_and_real_chunk_content() {
    let path =
        std::env::temp_dir().join(format!("knowledge-whitespace-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let text = format!("{} \n\t", "𠮷".repeat(1200));
    let (document, _) = db
        .import_knowledge(
            "resource-space",
            "空白尾部.txt",
            "text/plain",
            "hash-space",
            text.len() as i64,
            &text,
        )
        .unwrap();
    assert_eq!(document.char_count, 1203);
    assert_eq!(
        db.knowledge_text(&document.id).unwrap().as_deref(),
        Some(text.as_str())
    );
    let chunks = db.knowledge_chunks(Some(&document.id), None).unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[1].content, " \n\t");
    assert_eq!(chunks[1].start_char, Some(1200));
    assert_eq!(chunks[1].end_char, Some(1203));
    assert_eq!(
        chunks[1].version.as_deref(),
        Some(knowledge_text_version(&text).as_str())
    );
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.content.as_str())
            .collect::<String>(),
        text
    );
    for (index, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, index as i64);
        assert_eq!(
            db.find_memory(&chunk.memory_id).unwrap().unwrap().content,
            chunk.content
        );
    }
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn canonical_boundary_search_has_truthful_character_offset_and_no_fabricated_chunk_identity() {
    let path = std::env::temp_dir().join(format!("knowledge-search-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let query = "CROSS_BOUNDARY_NEEDLE";
    let text = format!("{}{query}{}", "𠮷".repeat(1195), "正文延续。".repeat(160));
    let (document, _) = db
        .import_knowledge(
            "resource-boundary",
            "边界资料.md",
            "text/markdown",
            "hash-boundary",
            text.len() as i64,
            &text,
        )
        .unwrap();
    let chunks = db.knowledge_chunks(Some(&document.id), None).unwrap();
    assert!(chunks.iter().all(|chunk| !chunk.content.contains(query)));
    assert!(db
        .knowledge_chunks(Some(&document.id), Some(query))
        .unwrap()
        .is_empty());
    let hits = db.search_knowledge(query).unwrap();
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.document_id, document.id);
    assert_eq!(hit.document_name, "边界资料.md");
    assert_eq!(hit.start_char, 1195 - 120);
    assert!(hit.content.contains(query));
    assert_eq!(
        text.chars()
            .skip(hit.start_char)
            .take(hit.content.chars().count())
            .collect::<String>(),
        hit.content
    );
    let json = serde_json::to_value(hit).unwrap();
    assert!(json.get("memory_id").is_none());
    assert!(json.get("chunk_index").is_none());
    let name_hits = db.search_knowledge("边界资料").unwrap();
    assert_eq!(name_hits.len(), 1);
    assert_eq!(name_hits[0].start_char, 0);
    assert!(text.starts_with(&name_hits[0].content));
    assert!(db.search_knowledge(" \n\t").unwrap().is_empty());
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn imported_chunks_do_not_crowd_out_personal_chat_memory_candidates() {
    let path =
        std::env::temp_dir().join(format!("knowledge-chat-memory-{}.db", uuid::Uuid::new_v4()));
    let db = crate::db::Database::new(&path).unwrap();
    let personal = db
        .create_memory(&crate::db::memories::CreateMemoryRequest {
            content: "ORCHID personal preference".into(),
            category: "preference".into(),
            source: "manual".into(),
            source_conversation_id: None,
            metadata: None,
        })
        .unwrap();
    db.conn()
        .execute(
            "UPDATE memories SET updated_at=1 WHERE id=?1",
            [&personal.id],
        )
        .unwrap();
    let text = "ORCHID document paragraph. ".repeat(30_000);
    db.import_knowledge(
        "r",
        "orchid.txt",
        "text/plain",
        "orchid-document",
        text.len() as i64,
        &text,
    )
    .unwrap();
    let result = db
        .retrieve_chat_memories(
            &crate::db::memories::RetrieveQuery {
                q: "ORCHID".into(),
                top_k: 6,
                category: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].memory.id, personal.id);
    assert!(!db.knowledge_context("ORCHID", 6).unwrap().is_empty());
    drop(db);
    std::fs::remove_file(path).unwrap();
}
