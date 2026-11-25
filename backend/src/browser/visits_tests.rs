use super::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio_util::sync::CancellationToken;
struct Host {
    tab: BrowserTab,
    calls: AtomicUsize,
    mode: u8,
}
#[async_trait::async_trait]
impl BrowserHost for Host {
    fn tabs(&self) -> Vec<BrowserTab> {
        vec![self.tab.clone()]
    }
    async fn dispatch(
        &self,
        request: BrowserRequest,
        _cancel: CancellationToken,
    ) -> Result<BrowserReply, BrowserError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            1 => Ok(BrowserReply::unknown(&request.operation_id, "uncertain")),
            2 => Err(BrowserError::new("blocked_origin", "Rejected")),
            _ => Ok(BrowserReply::verified(json!({"tab":self.tab}))),
        }
    }
}
fn fixture(
    mode: u8,
    saved: bool,
) -> (
    Arc<BrowserRuntime>,
    crate::db::Database,
    crate::db::browser_sites::ProjectSite,
    Arc<Host>,
    BrowserRequest,
) {
    let db = crate::db::Database::new(std::path::Path::new(":memory:")).unwrap();
    db.conn().execute("INSERT INTO workspaces(id,name,status,created_at,updated_at) VALUES('project-a','Project A','active',1,1)",[]).unwrap();
    let site = db
        .create_browser_site(
            &crate::db::browser_sites::CreateBrowserSite {
                workspace_id: "project-a".into(),
                title: "Fixture site".into(),
                purpose: String::new(),
                url: "http://localhost:3000/start".into(),
                startup_hint: String::new(),
                bookmarked: true,
            },
            2,
        )
        .unwrap();
    let runtime = Arc::new(BrowserRuntime::new());
    let resources = crate::shared::resource::ResourceService::new(
        db.clone_connection(),
        std::env::temp_dir().join(format!("browser-visit-resources-{}", uuid::Uuid::new_v4())),
        crate::shared::event::EventHub::new(4),
    );
    runtime.configure_evidence(db.clone_connection(), resources);
    let host = Arc::new(Host {
        tab: BrowserTab {
            session_id: "visible-session".into(),
            tab_id: "visible-tab".into(),
            workspace_id: if saved {
                Some(site.workspace_id.clone())
            } else {
                None
            },
            site_id: if saved { Some(site.id.clone()) } else { None },
            url: if mode == 3 {
                "http://other.example/result".into()
            } else {
                "http://localhost:3000/result?token=never-persist#private".into()
            },
            title: "Native page".into(),
            navigation_id: 7,
            mode: "embedded".into(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            error: if mode == 4 {
                Some("Website popup blocked".into())
            } else {
                None
            },
            ephemeral: true,
        },
        calls: AtomicUsize::new(0),
        mode,
    });
    runtime.install_host(host.clone()).unwrap();
    let request = BrowserRequest {
        operation_id: "open-once".into(),
        action: BrowserAction::Open,
        target: None,
        payload: if saved {
            json!({"url":site.url,"workspace_id":site.workspace_id,"site_id":site.id})
        } else {
            json!({"url":"http://localhost:3000/start"})
        },
        owner: None,
    };
    (runtime, db, site, host, request)
}
#[tokio::test]
async fn visible_verified_saved_site_open_records_once_and_duplicate_does_not_reopen() {
    let (runtime, db, site, host, request) = fixture(0, true);
    let first = runtime
        .execute(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(first.state, "verified");
    assert_eq!(first.data["history_projection"]["status"], "recorded");
    let again = runtime
        .execute(request, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        first.data["history_projection"]["visit_id"],
        again.data["history_projection"]["visit_id"]
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    let visits = db
        .list_browser_visits(Some("project-a"), Some(&site.id), 100)
        .unwrap();
    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].redacted_url, "http://localhost:3000/result");
    assert!(db
        .get_browser_site(&site.id)
        .unwrap()
        .last_visited_at
        .is_some());
}
#[tokio::test]
async fn visible_failed_unknown_generic_and_other_origin_do_not_create_saved_site_history() {
    for (mode, saved) in [(1, true), (2, true), (0, false), (3, true)] {
        let (runtime, db, site, _host, request) = fixture(mode, saved);
        let _ = runtime.execute(request, CancellationToken::new()).await;
        assert!(db.list_browser_visits(None, None, 100).unwrap().is_empty());
        assert!(db
            .get_browser_site(&site.id)
            .unwrap()
            .last_visited_at
            .is_none());
    }
}
#[tokio::test]
async fn visible_history_storage_failure_keeps_navigation_and_repairs_metadata_without_reopening() {
    let (runtime, db, site, host, request) = fixture(0, true);
    db.conn().execute_batch("CREATE TRIGGER browser_history_fail BEFORE INSERT ON browser_visits BEGIN SELECT RAISE(ABORT,'controlled history storage failure'); END;").unwrap();
    let first = runtime
        .execute(request.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(first.state, "verified");
    assert_eq!(first.data["history_projection"]["status"], "failed");
    let observed = first.data["history_projection"]["observed_at"].clone();
    assert!(db.list_browser_visits(None, None, 100).unwrap().is_empty());
    db.conn()
        .execute_batch("DROP TRIGGER browser_history_fail;")
        .unwrap();
    let repair = runtime.reconcile_saved_site_visits(Some("project-a"), Some(&site.id), 16);
    assert_eq!(repair["pending_count"], 0);
    assert_eq!(repair["issues"], json!([]));
    let duplicate = runtime
        .execute(request, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(duplicate.state, "verified");
    assert_eq!(duplicate.data["history_projection"]["status"], "recorded");
    assert_eq!(
        duplicate.data["history_projection"]["observed_at"],
        observed
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    assert_eq!(db.list_browser_visits(None, None, 100).unwrap().len(), 1);
}

#[tokio::test]
async fn visible_verified_navigation_with_blocked_popup_still_records_visit() {
    let (runtime, db, _site, _host, request) = fixture(4, true);
    let result = runtime
        .execute(request, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.state, "verified");
    assert_eq!(result.data["history_projection"]["status"], "recorded");
    assert_eq!(db.list_browser_visits(None, None, 100).unwrap().len(), 1);
}
