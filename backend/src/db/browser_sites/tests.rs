use super::*;
fn fixture() -> Database {
    let db = Database::new(std::path::Path::new(":memory:")).unwrap();
    for id in ["project-a", "project-b"] {
        db.conn().execute("INSERT INTO workspaces(id,name,status,created_at,updated_at) VALUES(?1,?1,'active',1,1)",[id]).unwrap();
    }
    db
}
fn input(project: &str) -> CreateBrowserSite {
    CreateBrowserSite {
        workspace_id: project.into(),
        title: "本地前端".into(),
        purpose: "开发预览".into(),
        url: "http://localhost:3000/?view=desktop#home".into(),
        startup_hint: "npm run dev（仅提示，不执行）".into(),
        bookmarked: true,
    }
}
fn visit(site: &ProjectSite) -> BrowserVisitInput {
    BrowserVisitInput {
        workspace_id: Some(site.workspace_id.clone()),
        site_id: Some(site.id.clone()),
        session_id: "session-one".into(),
        tab_id: "tab-one".into(),
        navigation_id: "1".into(),
        url: "http://localhost:3000/result?access_token=never-persist#private".into(),
        title: "受控开发页".into(),
    }
}
#[test]
fn browser_sites_migration_is_repeatable_and_preserves_existing_domains() {
    let db = fixture();
    db.initialize_browser_sites().unwrap();
    db.initialize_browser_sites().unwrap();
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=1083",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(db.list_workspaces().unwrap().len(), 2);
    let site = db.create_browser_site(&input("project-a"), 2).unwrap();
    db.initialize_browser_sites().unwrap();
    assert_eq!(db.get_browser_site(&site.id).unwrap().url, site.url);
    let foreign_keys:i64=db.conn().query_row("SELECT COUNT(*) FROM pragma_foreign_key_list('browser_project_sites') WHERE \"table\"='workspaces'",[],|r|r.get(0)).unwrap();
    assert_eq!(foreign_keys, 1);
}
#[test]
fn browser_sites_same_address_has_independent_project_identity() {
    let db = fixture();
    let a = db.create_browser_site(&input("project-a"), 2).unwrap();
    let b = db.create_browser_site(&input("project-b"), 3).unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(a.url, b.url);
    assert_eq!(a.port, 3000);
    assert_eq!(a.origin, "http://localhost:3000");
    assert_eq!(
        db.list_browser_sites(Some("project-a"), false, 20)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(db.list_browser_sites(None, false, 20).unwrap().len(), 2);
    assert_eq!(a.startup_hint, input("project-a").startup_hint);
}
#[test]
fn browser_sites_updates_reject_stale_revision_and_keep_workspace_immutable() {
    let db = fixture();
    let a = db.create_browser_site(&input("project-a"), 2).unwrap();
    let update = UpdateBrowserSite {
        title: Some("新标题".into()),
        expected_revision: Some(a.revision),
        ..Default::default()
    };
    let changed = db.update_browser_site(&a.id, &update, 3).unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.workspace_id, "project-a");
    assert!(matches!(
        db.update_browser_site(&a.id, &update, 4),
        Err(BrowserSiteError::Conflict)
    ));
    assert_eq!(db.get_browser_site(&a.id).unwrap().title, "新标题");
}
#[test]
fn browser_sites_archives_restore_and_workspace_guard_preserve_history() {
    let db = fixture();
    let site = db.create_browser_site(&input("project-a"), 2).unwrap();
    let recorded = db.note_browser_visit(&visit(&site), 3).unwrap();
    let archived = db
        .set_browser_site_archived(&site.id, true, Some(1), 4)
        .unwrap();
    assert_eq!(archived.status, "archived");
    assert!(db.list_browser_sites(None, false, 10).unwrap().is_empty());
    assert_eq!(db.list_browser_sites(None, true, 10).unwrap().len(), 1);
    assert!(matches!(
        db.update_browser_site(&site.id, &UpdateBrowserSite::default(), 5),
        Err(BrowserSiteError::Archived)
    ));
    db.set_browser_site_archived(&site.id, false, Some(2), 6)
        .unwrap();
    db.conn()
        .execute(
            "UPDATE workspaces SET status='archived' WHERE id='project-a'",
            [],
        )
        .unwrap();
    assert!(matches!(
        db.create_browser_site(&input("project-a"), 7),
        Err(BrowserSiteError::Archived)
    ));
    assert!(matches!(
        db.update_browser_site(&site.id, &UpdateBrowserSite::default(), 7),
        Err(BrowserSiteError::Archived)
    ));
    db.set_browser_site_archived(&site.id, true, None, 7)
        .unwrap();
    assert!(matches!(
        db.set_browser_site_archived(&site.id, false, None, 8),
        Err(BrowserSiteError::Archived)
    ));
    assert_eq!(
        db.list_browser_visits(None, None, 10).unwrap()[0].id,
        recorded.id
    );
}
#[test]
fn browser_visits_redact_secrets_deduplicate_and_reject_scope_collision() {
    let db = fixture();
    let site = db.create_browser_site(&input("project-a"), 2).unwrap();
    let data = visit(&site);
    let first = db.note_browser_visit(&data, 3).unwrap();
    let second = db.note_browser_visit(&data, 4).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.visited_at, 3);
    assert_eq!(first.redacted_url, "http://localhost:3000/result");
    assert!(!serde_json::to_string(&first)
        .unwrap()
        .contains("never-persist"));
    let mut other = data.clone();
    other.workspace_id = Some("project-b".into());
    assert!(db.note_browser_visit(&other, 5).is_err());
    other = data.clone();
    other.url = "http://localhost:3000/another".into();
    assert!(matches!(
        db.note_browser_visit(&other, 5),
        Err(BrowserSiteError::Conflict)
    ));
    assert_eq!(db.list_browser_visits(None, None, 10).unwrap().len(), 1);
    assert_eq!(
        db.get_browser_site(&site.id).unwrap().last_visited_at,
        Some(3)
    );
}
#[test]
fn browser_visits_are_bounded_and_project_filters_do_not_merge_equal_urls() {
    let db = fixture();
    let a = db.create_browser_site(&input("project-a"), 2).unwrap();
    let b = db.create_browser_site(&input("project-b"), 2).unwrap();
    for n in 0..4 {
        let mut data = visit(if n % 2 == 0 { &a } else { &b });
        data.navigation_id = n.to_string();
        db.note_browser_visit(&data, n + 3).unwrap();
    }
    assert_eq!(db.list_browser_visits(None, None, 2).unwrap().len(), 2);
    assert_eq!(
        db.list_browser_visits(Some("project-a"), None, 20)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.list_browser_visits(None, Some(&b.id), 20).unwrap().len(),
        2
    );
    assert!(db
        .note_browser_visit(
            &BrowserVisitInput {
                workspace_id: None,
                site_id: None,
                session_id: String::new(),
                tab_id: "t".into(),
                navigation_id: "n".into(),
                url: "https://example.test/".into(),
                title: String::new()
            },
            9
        )
        .is_err());
}
#[test]
fn browser_url_policy_rejects_credentials_internal_schemes_and_reserved_origins() {
    for raw in [
        "file:///C:/Windows/",
        "javascript:alert(1)",
        "tauri://localhost/",
        "http://tauri.localhost/",
        "http://tauri.localhost./",
        "http://localhost:9420/",
        "http://127.0.0.2:1420/",
        "http://[::1]:9420/",
        "https://user:pass@example.test/",
        "https://example.test/?access_token=value",
        "https://example.test/#id_token=value",
        "https://example.test/?client-secret=value",
        "https://example.test/\n",
    ] {
        assert!(normalize_site_url(raw).is_err(), "must reject {raw}");
    }
    let valid = normalize_site_url("http://localhost:3000/?view=desktop#home").unwrap();
    assert_eq!(valid.port, 3000);
    assert!(valid.url.contains("view=desktop"));
    assert_eq!(
        normalize_site_url("https://example.test/").unwrap().port,
        443
    );
    assert_eq!(
        redacted_visit_url("https://example.test/?token=sensitive#private").unwrap(),
        "https://example.test/"
    );
}
#[test]
fn browser_site_invalid_update_is_atomic_and_never_changes_saved_url() {
    let db = fixture();
    let a = db.create_browser_site(&input("project-a"), 2).unwrap();
    assert!(db
        .update_browser_site(
            &a.id,
            &UpdateBrowserSite {
                title: Some("changed".into()),
                url: Some("file:///tmp/private".into()),
                ..Default::default()
            },
            3
        )
        .is_err());
    let unchanged = db.get_browser_site(&a.id).unwrap();
    assert_eq!(unchanged.title, a.title);
    assert_eq!(unchanged.revision, 1);
    assert_eq!(unchanged.url, a.url);
}
