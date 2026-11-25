//! Keep system-browser link interception out of native website content.
use tauri::{ipc::Invoke, plugin::Plugin, AppHandle, Runtime, Webview, Window};

struct BrowserScopedShell<P>(P);

pub fn scoped_shell<R: Runtime>() -> impl Plugin<R> {
    BrowserScopedShell(tauri_plugin_shell::init::<R>())
}

impl<R: Runtime, P: Plugin<R>> Plugin<R> for BrowserScopedShell<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn initialize(
        &mut self,
        app: &AppHandle<R>,
        config: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.0.initialize(app, config)
    }

    fn initialization_script(&self) -> Option<String> {
        // Tauri installs native webview metadata before plugin scripts and webpage code.
        // The shell plugin otherwise prevents _blank links and invokes IPC in websites,
        // whose IPC is deliberately disabled. Native NewWindowRequested owns those links.
        self.0.initialization_script().map(|script| format!(
            "if (!window.__TAURI_INTERNALS__.metadata.currentWebview.label.startsWith('browser-content-')) {{\n{script}\n}}"
        ))
    }

    fn window_created(&mut self, window: Window<R>) {
        self.0.window_created(window);
    }
    fn webview_created(&mut self, webview: Webview<R>) {
        self.0.webview_created(webview);
    }
    fn on_navigation(&mut self, webview: &Webview<R>, url: &tauri::Url) -> bool {
        self.0.on_navigation(webview, url)
    }
    fn on_page_load(
        &mut self,
        webview: &Webview<R>,
        payload: &tauri::webview::PageLoadPayload<'_>,
    ) {
        self.0.on_page_load(webview, payload);
    }
    fn on_event(&mut self, app: &AppHandle<R>, event: &tauri::RunEvent) {
        self.0.on_event(app, event);
    }
    fn extend_api(&mut self, invoke: Invoke<R>) -> bool {
        self.0.extend_api(invoke)
    }
}
