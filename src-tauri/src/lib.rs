mod api;
mod books;
mod config;
mod models;
mod store;

use api::ReaderApi;
use models::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use store::Store;
use tauri::{Emitter, Manager, State};

struct Account {
    generation: u64,
    api: Option<Arc<ReaderApi>>,
}
struct AppState {
    account: Mutex<Account>,
    store: Mutex<Store>,
    sync_lock: tokio::sync::Mutex<()>,
    data_dir: PathBuf,
}

fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new("io.quietreader.desktop", "readwise-token")
        .map_err(|_| "Windows Credential Manager is unavailable.".into())
}

fn client(token: String, app: &tauri::AppHandle) -> Result<Arc<ReaderApi>, String> {
    let handle = app.clone();
    ReaderApi::new(
        token,
        Arc::new(move |message| {
            let _ = handle.emit(
                "sync-progress",
                SyncProgress {
                    message,
                    completed: 0,
                },
            );
        }),
    )
    .map(Arc::new)
}

fn session(state: &AppState) -> Result<(u64, Arc<ReaderApi>), String> {
    let account = state
        .account
        .lock()
        .map_err(|_| "Account state is unavailable.")?;
    Ok((
        account.generation,
        account
            .api
            .clone()
            .ok_or("Connect your Readwise account first.")?,
    ))
}

fn current<T>(
    state: &AppState,
    generation: u64,
    action: impl FnOnce(&mut Store) -> Result<T, String>,
) -> Result<T, String> {
    let account = state
        .account
        .lock()
        .map_err(|_| "Account state is unavailable.")?;
    if generation != account.generation || account.api.is_none() {
        return Err("The account changed; this request was cancelled.".into());
    }
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Library storage is unavailable.")?;
    action(&mut store)
}

#[tauri::command]
fn bootstrap(state: State<AppState>) -> Result<Bootstrap, String> {
    let account = state
        .account
        .lock()
        .map_err(|_| "Account state is unavailable.")?;
    let connected = account.api.is_some();
    let store = state
        .store
        .lock()
        .map_err(|_| "Library storage is unavailable.")?;
    let library = if connected {
        store.library()?
    } else {
        LibrarySnapshot {
            documents: vec![],
            last_synced: None,
        }
    };
    let views = config::load(&state.data_dir.join("views.json"));
    Ok(Bootstrap {
        connected,
        documents: library.documents,
        last_synced: library.last_synced,
        settings: store.settings()?,
        views: views.views,
        config_path: views.config_path,
        config_error: views.config_error,
    })
}

#[tauri::command]
async fn connect(
    token: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Bootstrap, String> {
    let token = token.trim().to_string();
    if token.is_empty() || token.len() > 512 {
        return Err("Enter a valid Readwise access token.".into());
    }
    let api = client(token.clone(), &app)?;
    api.validate_token().await?;
    replace_account(&state, &credential()?, &token, api)?;
    bootstrap(state)
}

fn replace_account(
    state: &AppState,
    entry: &keyring::Entry,
    token: &str,
    api: Arc<ReaderApi>,
) -> Result<(), String> {
    let mut account = state
        .account
        .lock()
        .map_err(|_| "Account state is unavailable.")?;
    let mut store = state
        .store
        .lock()
        .map_err(|_| "Library storage is unavailable.")?;
    account.generation += 1;
    account.api = None;
    store.clear_account()?;
    books::clear(&state.data_dir)?;
    // A crash before this write leaves the old token with an empty cache, never a new token with old content.
    entry
        .set_password(token)
        .map_err(|_| "Cannot save the token in Windows Credential Manager.")?;
    account.api = Some(api);
    Ok(())
}

#[tauri::command]
fn disconnect(state: State<AppState>) -> Result<(), String> {
    let mut account = state
        .account
        .lock()
        .map_err(|_| "Account state is unavailable.")?;
    match credential()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err("Cannot remove the token from Windows Credential Manager.".into()),
    }
    account.generation += 1;
    account.api = None;
    state
        .store
        .lock()
        .map_err(|_| "Library storage is unavailable.")?
        .clear_account()?;
    books::clear(&state.data_dir)
}

#[tauri::command]
async fn sync_library(
    full: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<LibrarySnapshot, String> {
    let (generation, api) = session(&state)?;
    let _sync = state.sync_lock.lock().await;
    let previous = current(&state, generation, |store| store.library())?.last_synced;
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut documents = Vec::new();
    for (filter, value) in [
        ("location", "new"),
        ("location", "later"),
        ("location", "shortlist"),
        ("location", "archive"),
        ("category", "highlight"),
    ] {
        let mut cursor: Option<String> = None;
        let mut seen_cursors = std::collections::HashSet::new();
        loop {
            current(&state, generation, |_| Ok(()))?;
            let mut params = vec![(filter, value.to_string()), ("limit", "100".to_string())];
            if !full {
                if let Some(after) = &previous {
                    params.push(("updatedAfter", after.clone()));
                }
            }
            if let Some(value) = &cursor {
                params.push(("pageCursor", value.clone()));
            }
            let page = api.list_page(&params).await?;
            current(&state, generation, |store| {
                store.upsert_partial(&page.results, &started_at)
            })?;
            documents.extend(page.results);
            let _ = app.emit(
                "sync-progress",
                SyncProgress {
                    message: format!(
                        "Syncing {} · {} items",
                        if filter == "category" {
                            "highlights"
                        } else {
                            "library"
                        },
                        documents.len()
                    ),
                    completed: documents.len(),
                },
            );
            match page.next_page_cursor {
                Some(next) if !next.is_empty() => {
                    if !seen_cursors.insert(next.clone()) {
                        return Err(
                            "Reader returned a repeated pagination cursor. Try a full refresh."
                                .into(),
                        );
                    }
                    cursor = Some(next);
                }
                _ => break,
            }
        }
    }
    current(&state, generation, |store| {
        store.apply_sync(&documents, &started_at, full || previous.is_none())?;
        store.library()
    })
}

#[tauri::command]
async fn read_document(
    id: String,
    chapter: Option<usize>,
    state: State<'_, AppState>,
) -> Result<ReadingDocument, String> {
    let (generation, api) = session(&state)?;
    let (document, position, highlights, cached) = current(&state, generation, |store| {
        Ok((
            store.document(&id)?,
            store.position(&id)?,
            store.highlights(&id)?,
            store.content(&id)?,
        ))
    })?;
    if document.category == "epub" {
        let chosen = chapter.or_else(|| position.as_ref().map(|p| p.chapter));
        let cache_path = books::path(&state.data_dir, &id)?;
        let bytes = match books::cached(&cache_path)? {
            Some(bytes) => bytes,
            None => {
                let page = api
                    .list_page(&[("id", id.clone()), ("withRawSourceUrl", "true".into())])
                    .await?;
                let source = page
                    .results
                    .first()
                    .and_then(|doc| doc.raw_source_url.as_deref())
                    .ok_or("Reader has no downloadable source for this book. Open it in Reader.")?;
                let bytes = books::download(source).await?;
                current(&state, generation, |_| books::save(&cache_path, &bytes))?;
                bytes
            }
        };
        let (html, chapters, chapter) = books::read(bytes, chosen, document.reading_progress)?;
        current(&state, generation, |_| {
            Ok(ReadingDocument {
                document,
                html,
                highlights,
                position,
                chapters,
                chapter,
            })
        })
    } else {
        let html = match cached {
            Some(html) => html,
            None => {
                let page = api
                    .list_page(&[("id", id.clone()), ("withHtmlContent", "true".into())])
                    .await?;
                let html = page
                    .results
                    .first()
                    .and_then(|doc| doc.html_content.clone())
                    .filter(|html| !html.trim().is_empty())
                    .ok_or("Reader has no readable HTML for this item yet. Open it in Reader.")?;
                current(&state, generation, |store| store.save_content(&id, &html))?;
                html
            }
        };
        current(&state, generation, |_| {
            Ok(ReadingDocument {
                document,
                html,
                highlights,
                position,
                chapters: vec![],
                chapter: 0,
            })
        })
    }
}

#[tauri::command]
async fn archive_document(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let (generation, api) = session(&state)?;
    current(&state, generation, |store| store.document(&id))?;
    api.archive(&id).await?;
    current(&state, generation, |store| store.set_archived(&id))
}

#[tauri::command]
async fn create_highlight(
    id: String,
    text: String,
    offset: Option<u64>,
    chapter: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Highlight, String> {
    let (generation, api) = session(&state)?;
    current(&state, generation, |store| store.document(&id))?;
    let highlight_id = api.highlight(&id, &text).await?;
    current(&state, generation, |store| {
        store.save_highlight(&id, &highlight_id, &text, offset, chapter)?;
        Ok(Highlight {
            id: highlight_id,
            text,
            offset,
            chapter,
        })
    })
}

#[tauri::command]
fn save_position(id: String, position: Position, state: State<AppState>) -> Result<(), String> {
    let (generation, _) = session(&state)?;
    current(&state, generation, |store| {
        store.save_position(&id, &position)
    })
}

#[tauri::command]
fn save_settings(settings: Settings, state: State<AppState>) -> Result<(), String> {
    state
        .store
        .lock()
        .map_err(|_| "Library storage is unavailable.")?
        .save_settings(&settings)
}

#[tauri::command]
fn reload_views(state: State<AppState>) -> ViewsConfig {
    config::load(&state.data_dir.join("views.json"))
}

#[tauri::command]
fn open_config(state: State<AppState>) -> Result<(), String> {
    config::open(&state.data_dir.join("views.json"))
}

#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    let url = url::Url::parse(&url).map_err(|_| "This link is invalid.")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only ordinary HTTP and HTTPS links can be opened.".into());
    }
    open::that(url.as_str()).map_err(|_| "Windows could not open this link in your browser.".into())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let api = match credential()?.get_password() {
                Ok(token) => Some(client(token, app.handle())?),
                Err(keyring::Error::NoEntry) => None,
                Err(_) => return Err("Cannot read Windows Credential Manager.".into()),
            };
            app.manage(AppState {
                account: Mutex::new(Account { generation: 0, api }),
                store: Mutex::new(Store::open(&data_dir.join("library.db"))?),
                sync_lock: tokio::sync::Mutex::new(()),
                data_dir,
            });
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Quiet Reader")
            .inner_size(1280.0, 880.0)
            .min_inner_size(720.0, 540.0)
            .theme(Some(tauri::Theme::Dark))
            .background_color(tauri::webview::Color(0, 0, 0, 255))
            .on_navigation(|url| {
                matches!(
                    (url.scheme(), url.host_str()),
                    ("http" | "https", Some("tauri.localhost")) | ("tauri", Some("localhost"))
                ) || (cfg!(debug_assertions)
                    && url.host_str() == Some("127.0.0.1")
                    && url.port() == Some(1420))
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            connect,
            disconnect,
            sync_library,
            read_document,
            archive_document,
            create_highlight,
            save_position,
            save_settings,
            reload_views,
            open_config,
            open_external
        ])
        .run(tauri::generate_context!())
        .expect("Quiet Reader could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_cache_clear_never_pairs_new_credentials_with_old_content() {
        struct TestCredential(keyring::Entry);
        impl Drop for TestCredential {
            fn drop(&mut self) {
                let _ = self.0.delete_credential();
            }
        }
        let unique = format!(
            "quiet-reader-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let entry = TestCredential(keyring::Entry::new(&unique, "fixture").unwrap());
        entry.0.set_password("original-fixture-token").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.db");
        let mut store = Store::open(&path).unwrap();
        let document = serde_json::from_value(
            serde_json::json!({"id":"old-article", "category":"article", "location":"new"}),
        )
        .unwrap();
        store
            .apply_sync(&[document], "2000-01-01T00:00:00Z", true)
            .unwrap();
        let state = AppState {
            account: Mutex::new(Account {
                generation: 0,
                api: None,
            }),
            store: Mutex::new(store),
            sync_lock: tokio::sync::Mutex::new(()),
            data_dir: directory.path().to_path_buf(),
        };
        let failure = rusqlite::Connection::open(&path).unwrap();
        failure.execute_batch("CREATE TRIGGER fail_clear BEFORE DELETE ON documents BEGIN SELECT RAISE(ABORT, 'fixture storage failure'); END;").unwrap();
        let api = Arc::new(ReaderApi::new("new-fixture-token".into(), Arc::new(|_| {})).unwrap());
        assert!(replace_account(&state, &entry.0, "new-fixture-token", api.clone()).is_err());
        assert_eq!(
            state
                .store
                .lock()
                .unwrap()
                .library()
                .unwrap()
                .documents
                .len(),
            1
        );
        assert_eq!(entry.0.get_password().unwrap(), "original-fixture-token");
        assert!(state.account.lock().unwrap().api.is_none());
        failure.execute_batch("DROP TRIGGER fail_clear").unwrap();
        replace_account(&state, &entry.0, "new-fixture-token", api).unwrap();
        assert!(state
            .store
            .lock()
            .unwrap()
            .library()
            .unwrap()
            .documents
            .is_empty());
        assert_eq!(entry.0.get_password().unwrap(), "new-fixture-token");
        assert!(state.account.lock().unwrap().api.is_some());
    }

    #[test]
    fn stale_account_work_cannot_read_or_write_the_new_accounts_store() {
        let directory = tempfile::tempdir().unwrap();
        let api = ReaderApi::new("fixture-token".into(), Arc::new(|_| {})).unwrap();
        let state = AppState {
            account: Mutex::new(Account {
                generation: 2,
                api: Some(Arc::new(api)),
            }),
            store: Mutex::new(Store::open(&directory.path().join("library.db")).unwrap()),
            sync_lock: tokio::sync::Mutex::new(()),
            data_dir: directory.path().to_path_buf(),
        };
        let called = std::cell::Cell::new(false);
        assert!(current(&state, 1, |_| {
            called.set(true);
            Ok(())
        })
        .is_err());
        assert!(!called.get());
        assert!(current(&state, 2, |store| store.settings()).is_ok());
        state.account.lock().unwrap().api = None;
        assert!(current(&state, 2, |_| {
            called.set(true);
            Ok(())
        })
        .is_err());
        assert!(!called.get());
    }
}
