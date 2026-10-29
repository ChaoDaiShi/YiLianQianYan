use super::*;
use crate::{
    db::LlmModelInput,
    integrations::secret::{
        InMemorySecretStore, SecretRef, SecretStore, SecretStoreError, SecretStoreStatus,
    },
    safety::ControlSession,
};
use async_trait::async_trait;
use secrecy::SecretString;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct TrackedStore {
    inner: InMemorySecretStore,
    writes: AtomicUsize,
}
#[async_trait]
impl SecretStore for TrackedStore {
    async fn put(&self, key: &SecretRef, value: SecretString) -> Result<(), SecretStoreError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.put(key, value).await
    }
    async fn get(&self, key: &SecretRef) -> Result<Option<SecretString>, SecretStoreError> {
        self.inner.get(key).await
    }
    async fn delete(&self, key: &SecretRef) -> Result<(), SecretStoreError> {
        self.inner.delete(key).await
    }
    async fn status(&self) -> SecretStoreStatus {
        self.inner.status().await
    }
}
fn server(store: Arc<TrackedStore>) -> AppServer {
    let root = std::env::temp_dir().join(format!("credential-reuse-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    AppServer::new_with_control_session_and_store(
        &root.join("app.db"),
        root.to_str().unwrap(),
        ControlSession::new("r".repeat(64)).unwrap(),
        store,
    )
    .unwrap()
}
fn model(id: &str, key: &str) -> LlmModelInput {
    LlmModelInput {
        id: id.into(),
        provider: "deepseek".into(),
        label: id.into(),
        model: "deepseek-flash".into(),
        base_url: "https://example.invalid/v1".into(),
        api_format: "openai".into(),
        api_key_ref: key.into(),
        api_key_env: String::new(),
        temperature: 0.0,
        max_tokens: 1024,
        invoke_timeout_ms: 5000,
    }
}
fn account(id: &str) -> ProviderAccount {
    ProviderAccount {
        id: id.into(),
        provider_id: "deepseek".into(),
        label: id.into(),
        base_url: "https://example.invalid/v1".into(),
        credential_id: None,
        profile_ids: vec![],
    }
}
fn input(account: ProviderAccount, action: &str, source: Option<&str>) -> AccountInput {
    AccountInput {
        account,
        credential_action: Some(action.into()),
        credential_model_id: source.map(str::to_owned),
        credential_fields: Default::default(),
    }
}

#[tokio::test]
async fn new_empty_account_does_not_invent_a_credential_or_borrow_another_model_key() {
    let store = Arc::new(TrackedStore::default());
    let s = server(store.clone());
    store
        .put(
            &SecretRef::new("llm.model.existing"),
            SecretString::from("synthetic-existing-value"),
        )
        .await
        .unwrap();
    s.db.create_llm_model(&model("existing", "llm.model.existing"))
        .unwrap();
    let saved = persist_account(&s, input(account("empty"), "keep", None))
        .await
        .unwrap();
    assert!(saved.credential_id.is_none());
    let view = account_view(&s, saved).await;
    assert_eq!(view.credential_status, "missing");
    assert!(!view.credential_configured);
    assert_eq!(store.writes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn account_view_checks_the_saved_secret_instead_of_reference_presence() {
    let store = Arc::new(TrackedStore::default());
    let s = server(store.clone());
    let mut a = account("selected");
    a.credential_id = Some("llm.account.absent".into());
    let missing = account_view(&s, a.clone()).await;
    assert_eq!(missing.credential_status, "missing");
    assert!(!missing.credential_configured);
    store
        .put(
            &SecretRef::new("llm.account.absent"),
            SecretString::from("synthetic-visible-only-in-store"),
        )
        .await
        .unwrap();
    let configured = account_view(&s, a).await;
    assert_eq!(configured.credential_status, "configured");
    assert!(configured.credential_configured);
    let response = serde_json::to_string(&configured).unwrap();
    assert!(!response.contains("synthetic-visible-only-in-store"));
}

#[tokio::test]
async fn explicit_reuse_updates_the_linked_flash_profile_without_copying_secret_values() {
    let store = Arc::new(TrackedStore::default());
    let s = server(store.clone());
    let reference = "llm.model.saved-chat";
    store
        .put(
            &SecretRef::new(reference),
            SecretString::from("synthetic-shared-value"),
        )
        .await
        .unwrap();
    s.db.create_llm_model(&model("chat", reference)).unwrap();
    s.db.create_llm_model(&model("flash", "llm.account.empty"))
        .unwrap();
    let mut a = account("vision-account");
    a.credential_id = Some("llm.account.empty".into());
    a.profile_ids = vec!["flash".into()];
    s.db.save_provider_account(&a).unwrap();
    let saved = persist_account(&s, input(a, "reuse_model", Some("chat")))
        .await
        .unwrap();
    assert_eq!(saved.credential_id.as_deref(), Some(reference));
    assert_eq!(
        s.db.get_llm_model("flash").unwrap().unwrap().api_key_ref,
        reference
    );
    assert_eq!(
        s.db.get_llm_model("chat").unwrap().unwrap().api_key_ref,
        reference
    );
    assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    let view = account_view(&s, saved).await;
    assert_eq!(view.credential_status, "configured");
    assert!(!serde_json::to_value(view)
        .unwrap()
        .to_string()
        .contains("synthetic-shared-value"));
}

#[tokio::test]
async fn reuse_is_limited_to_the_selected_provider_and_api_address() {
    let store = Arc::new(TrackedStore::default());
    let s = server(store.clone());
    store
        .put(
            &SecretRef::new("llm.model.saved"),
            SecretString::from("synthetic-value"),
        )
        .await
        .unwrap();
    s.db.create_llm_model(&model("saved", "llm.model.saved"))
        .unwrap();
    let mut other = account("other-provider");
    other.provider_id = "openai".into();
    assert!(
        persist_account(&s, input(other, "reuse_model", Some("saved")))
            .await
            .is_err()
    );
    let mut other = account("other-address");
    other.base_url = "https://another.invalid/v1".into();
    assert!(
        persist_account(&s, input(other, "reuse_model", Some("saved")))
            .await
            .is_err()
    );
    assert!(s.db.list_provider_accounts().unwrap().is_empty());
    assert_eq!(store.writes.load(Ordering::SeqCst), 1);
    let dto:AccountInput=serde_json::from_value(json!({"id":"new","provider_id":"deepseek","label":"New","base_url":"https://example.invalid/v1","credential_id":null,"profile_ids":[],"credential_action":"keep"})).unwrap();
    assert!(dto.credential_model_id.is_none());
}
