use super::*;
use std::{cell::RefCell, collections::HashMap};
use webview2_com::{
    ClearBrowsingDataCompletedHandler, FindStartCompletedHandler,
    Microsoft::Web::WebView2::Win32::*,
};
use windows::core::{Interface, HSTRING};

thread_local! {
    static FIND_QUERIES: RefCell<HashMap<String,String>> = RefCell::new(HashMap::new());
}

pub(super) fn forget(tab_id: Option<&str>) {
    FIND_QUERIES.with(|all| match tab_id {
        Some(id) => {
            all.borrow_mut().remove(id);
        }
        None => all.borrow_mut().clear(),
    });
}

fn find_result(find: &ICoreWebView2Find, query: &str) -> windows::core::Result<Value> {
    unsafe {
        let mut count = 0;
        let mut index = -1;
        find.MatchCount(&mut count)?;
        find.ActiveMatchIndex(&mut index)?;
        Ok(json!({"query":query,"matched":count>0,"match_count":count,"active_match_index":index}))
    }
}

pub(super) async fn execute(
    view: &Webview,
    command: &str,
    payload: Value,
) -> Result<Value, String> {
    if command == "zoom" {
        let factor = payload
            .get("factor")
            .and_then(Value::as_f64)
            .ok_or("Zoom factor is required")?;
        if !factor.is_finite() || !(0.25..=5.0).contains(&factor) {
            return Err("Zoom must be between 25% and 500%".into());
        }
        view.set_zoom(factor).map_err(|e| e.to_string())?;
        // Read back the native controller value; request acknowledgement is not zoom evidence.
        let (tx, rx) = tokio::sync::oneshot::channel();
        view.with_webview(move |native| {
            let result = unsafe {
                let mut factor = 1.0;
                native
                    .controller()
                    .ZoomFactor(&mut factor)
                    .map(|_| json!({"factor":factor}))
                    .map_err(|e| e.to_string())
            };
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
        return rx
            .await
            .map_err(|_| "Native zoom acknowledgement was interrupted".to_string())?;
    }
    if command == "clear_data" {
        return clear_data(view).await;
    }
    if command != "find" {
        return Err("Unsupported browser UI command".into());
    }
    let query = payload
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let backwards = payload
        .get("backwards")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let label = view.label().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(tx)));
    view.with_webview(move |native| {
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                let find = native.controller().CoreWebView2()?.cast::<ICoreWebView2_28>()?.Find()?;
                if query.is_empty() {
                    find.Stop()?;
                    FIND_QUERIES.with(|all| all.borrow_mut().remove(&label));
                    if let Some(tx) = sender.lock().unwrap().take() { let _ = tx.send(Ok(json!({"query":"","matched":false,"match_count":0,"active_match_index":-1}))); }
                    return Ok(());
                }
                let same = FIND_QUERIES.with(|all| all.borrow().get(&label)==Some(&query));
                if same {
                    if backwards { find.FindPrevious()?; } else { find.FindNext()?; }
                    if let Some(tx) = sender.lock().unwrap().take() { let _ = tx.send(find_result(&find,&query).map_err(|e| e.to_string())); }
                    return Ok(());
                }
                let options = native.environment().cast::<ICoreWebView2Environment15>()?.CreateFindOptions()?;
                options.SetFindTerm(&HSTRING::from(&query))?;
                options.SetIsCaseSensitive(false)?;
                options.SetShouldHighlightAllMatches(true)?;
                options.SetSuppressDefaultFindDialog(true)?;
                let completed_sender = sender.clone();
                let completed_find = find.clone();
                find.Start(&options, &FindStartCompletedHandler::create(Box::new(move |result| {
                    let result = result.and_then(|_| {
                        if backwards { completed_find.FindPrevious()?; }
                        FIND_QUERIES.with(|all| all.borrow_mut().insert(label.clone(), query.clone()));
                        find_result(&completed_find, &query)
                    }).map_err(|e| e.to_string());
                    if let Some(tx) = completed_sender.lock().unwrap().take() { let _ = tx.send(result); }
                    Ok(())
                })))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(tx) = sender.lock().unwrap().take() { let _ = tx.send(Err(format!("Native find is unavailable: {error}"))); }
        }
    }).map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(|_| "Native find timed out".to_string())?
        .map_err(|_| "Native find acknowledgement was interrupted".to_string())?
}

async fn clear_data(view: &Webview) -> Result<Value, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(tx)));
    view.with_webview(move |native| {
        let result = (|| -> windows::core::Result<()> {
            unsafe {
                let profile = native
                    .controller()
                    .CoreWebView2()?
                    .cast::<ICoreWebView2_13>()?
                    .Profile()?
                    .cast::<ICoreWebView2Profile2>()?;
                let completed_sender = sender.clone();
                profile.ClearBrowsingDataAll(&ClearBrowsingDataCompletedHandler::create(
                    Box::new(move |result| {
                        if let Some(tx) = completed_sender.lock().unwrap().take() {
                            let _ = tx.send(
                                result
                                    .map(|_| json!({"cleared":true}))
                                    .map_err(|e| e.to_string()),
                            );
                        }
                        Ok(())
                    }),
                ))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(tx) = sender.lock().unwrap().take() {
                let _ = tx.send(Err(error.to_string()));
            }
        }
    })
    .map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .map_err(|_| "Clearing browser data is still pending; inspect before retrying".to_string())?
        .map_err(|_| "Clear browser data acknowledgement was interrupted".to_string())?
}
