use super::*;

#[test]
fn folder_memberships_preserve_relative_paths_across_deduplication_and_restart() {
    let path =
        std::env::temp_dir().join(format!("knowledge-collections-{}.db", uuid::Uuid::new_v4()));
    let db = Database::new(&path).unwrap();
    let first = db.ensure_knowledge_collection("课程资料").unwrap();
    let second = db.ensure_knowledge_collection("项目资料").unwrap();
    assert_eq!(
        db.ensure_knowledge_collection("课程资料").unwrap().id,
        first.id
    );
    let (document, created) = db
        .import_knowledge_in_collection(
            "source-1",
            "a.txt",
            "text/plain",
            "shared-hash",
            12,
            "共同知识证据",
            Some(&KnowledgeImportLocation {
                collection_id: first.id.clone(),
                relative_path: "课程资料/章节/a.txt".into(),
            }),
        )
        .unwrap();
    assert!(created);
    let (same, created) = db
        .import_knowledge_in_collection(
            "source-2",
            "copy.txt",
            "text/plain",
            "shared-hash",
            12,
            "共同知识证据",
            Some(&KnowledgeImportLocation {
                collection_id: second.id.clone(),
                relative_path: "项目资料/参考/copy.txt".into(),
            }),
        )
        .unwrap();
    assert!(!created);
    assert_eq!(same.id, document.id);
    assert_eq!(db.knowledge_documents().unwrap().len(), 1);
    assert_eq!(
        db.knowledge_documents_for_collection(Some(&first.id))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.knowledge_documents_for_collection(Some(&second.id))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.search_knowledge_in_collection("证据", Some(&second.id))
            .unwrap()[0]
            .document_id,
        document.id
    );
    drop(db);
    let db = Database::new(&path).unwrap();
    let memberships = db.knowledge_memberships(Some(&document.id)).unwrap();
    assert_eq!(memberships.len(), 2);
    assert!(memberships
        .iter()
        .any(|membership| membership.relative_path == "课程资料/章节/a.txt"));
    assert!(memberships
        .iter()
        .any(|membership| membership.relative_path == "项目资料/参考/copy.txt"));
    db.delete_knowledge_document(&document.id).unwrap();
    assert!(db.knowledge_memberships(None).unwrap().is_empty());
    assert_eq!(db.knowledge_collections().unwrap().len(), 2);
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_import_location_rolls_back_document_and_path_reimport_preserves_old_document() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let collection = db.ensure_knowledge_collection("资料").unwrap();
    let invalid = KnowledgeImportLocation {
        collection_id: collection.id.clone(),
        relative_path: "../outside.txt".into(),
    };
    assert!(db
        .import_knowledge_in_collection(
            "resource",
            "a.txt",
            "text/plain",
            "bad-hash",
            10,
            "不会写入",
            Some(&invalid)
        )
        .is_err());
    assert!(db.knowledge_documents().unwrap().is_empty());
    let location = KnowledgeImportLocation {
        collection_id: collection.id.clone(),
        relative_path: "资料/a.txt".into(),
    };
    let (old, _) = db
        .import_knowledge_in_collection(
            "resource1",
            "a.txt",
            "text/plain",
            "old-hash",
            10,
            "旧版资料",
            Some(&location),
        )
        .unwrap();
    let (new, _) = db
        .import_knowledge_in_collection(
            "resource2",
            "a.txt",
            "text/plain",
            "new-hash",
            10,
            "新版资料",
            Some(&location),
        )
        .unwrap();
    assert_eq!(db.knowledge_documents().unwrap().len(), 2);
    assert!(db.find_knowledge_document(&old.id).unwrap().is_some());
    assert_eq!(
        db.knowledge_documents_for_collection(Some(&collection.id))
            .unwrap()[0]
            .id,
        new.id
    );
    assert_eq!(db.knowledge_collections().unwrap()[0].path_count, 1);
}

#[test]
fn full_large_text_above_two_million_characters_remains_searchable_at_the_tail() {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    let input = format!("{}END_OF_LARGE_KNOWLEDGE_DOCUMENT", "a".repeat(2_000_500));
    let extracted = crate::modules::resource::extract_document_text(
        "large.txt",
        "text/plain",
        input.as_bytes(),
    )
    .unwrap();
    let (document, _) = db
        .import_knowledge(
            "large-resource",
            "large.txt",
            "text/plain",
            "large-hash",
            input.len() as i64,
            &extracted,
        )
        .unwrap();
    assert_eq!(document.char_count as usize, input.chars().count());
    assert_eq!(db.knowledge_text(&document.id).unwrap().unwrap(), input);
    let hits = db
        .search_knowledge("END_OF_LARGE_KNOWLEDGE_DOCUMENT")
        .unwrap();
    assert_eq!(hits[0].document_id, document.id);
    assert!(hits[0].start_char > 2_000_000);
    assert!(hits[0].content.ends_with("END_OF_LARGE_KNOWLEDGE_DOCUMENT"));
}
