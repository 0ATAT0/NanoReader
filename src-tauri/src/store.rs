use crate::models::{ApiDocument, Document, Highlight, LibrarySnapshot, Position, Settings};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::{path::Path, time::Duration};

const CACHE_BYTES: usize = 128 * 1024 * 1024;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Could not create the local reader data folder.".to_string())?;
        }
        let connection = Connection::open(path).map_err(storage_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(storage_error)?;
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(storage_error)?;
        if version > 1 {
            return Err("The local library was created by a newer app. Update NanoReader.".into());
        }
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
             PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA wal_autocheckpoint = 256;
             PRAGMA journal_size_limit = 4194304;
             CREATE TABLE IF NOT EXISTS documents (
                 id TEXT PRIMARY KEY, data TEXT NOT NULL, local_mutation INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS highlights (
                 id TEXT PRIMARY KEY,
                 parent_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                 data TEXT NOT NULL, local_mutation INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS highlights_parent ON highlights(parent_id);
             CREATE TABLE IF NOT EXISTS content (
                 id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
                 html TEXT NOT NULL, bytes INTEGER NOT NULL, accessed INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS positions (
                 id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
                 data TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS sync_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             PRAGMA user_version = 1;",
            )
            .map_err(storage_error)?;
        Ok(Self { connection })
    }

    pub fn library(&self) -> Result<LibrarySnapshot, String> {
        let mut statement = self
            .connection
            .prepare("SELECT data FROM documents ORDER BY id")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        let mut documents = Vec::new();
        for row in rows {
            documents.push(parse(&row.map_err(storage_error)?)?);
        }
        let last_synced = self
            .connection
            .query_row(
                "SELECT value FROM sync_state WHERE key = 'last_synced'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        Ok(LibrarySnapshot {
            documents,
            last_synced,
        })
    }

    /// Only completed paging advances the sync cursor and reconciles deleted documents.
    pub fn apply_sync(
        &mut self,
        docs: &[ApiDocument],
        started_at: &str,
        full: bool,
    ) -> Result<(), String> {
        self.sync(docs, started_at, full, true)
    }

    pub fn upsert_partial(&mut self, docs: &[ApiDocument], started_at: &str) -> Result<(), String> {
        self.sync(docs, started_at, false, false)
    }

    fn sync(
        &mut self,
        docs: &[ApiDocument],
        started_at: &str,
        full: bool,
        advance_cursor: bool,
    ) -> Result<(), String> {
        let started = chrono::DateTime::parse_from_rfc3339(started_at)
            .map_err(|_| "The library sync timestamp is invalid.".to_string())?
            .timestamp_micros();
        let transaction = self.connection.transaction().map_err(storage_error)?;
        transaction
            .execute_batch(
                "CREATE TEMP TABLE IF NOT EXISTS sync_ids (id TEXT PRIMARY KEY);
             DELETE FROM sync_ids;",
            )
            .map_err(storage_error)?;
        for api in docs {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO sync_ids(id) VALUES (?1)",
                    [&api.document.id],
                )
                .map_err(storage_error)?;
        }
        for api in docs.iter().filter(|d| d.parent_id.is_none()) {
            if matches!(api.document.category.as_str(), "highlight" | "note") {
                continue;
            }
            let mut document = api.document.clone();
            let previous: Option<(String, i64)> = transaction
                .query_row(
                    "SELECT data, local_mutation FROM documents WHERE id = ?1",
                    [&document.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;
            let mutation = match previous {
                Some((data, mutation)) => {
                    let previous: Document = parse(&data)?;
                    if previous.updated_at != document.updated_at {
                        transaction
                            .execute("DELETE FROM content WHERE id = ?1", [&document.id])
                            .map_err(storage_error)?;
                    }
                    if mutation > started {
                        document.location = previous.location;
                        mutation
                    } else {
                        0
                    }
                }
                _ => 0,
            };
            transaction.execute(
                "INSERT INTO documents(id, data, local_mutation) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET data = excluded.data, local_mutation = excluded.local_mutation",
                params![document.id, json(&document)?, mutation],
            ).map_err(storage_error)?;
        }
        for api in docs.iter().filter(|d| d.document.category == "highlight") {
            let Some(parent) = api.parent_id.as_deref() else {
                continue;
            };
            let Some(text) = api
                .content
                .as_deref()
                .filter(|text| !text.trim().is_empty())
            else {
                // Empty/image highlights have no passage to render; retain existing local text.
                continue;
            };
            let parent_exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM documents WHERE id = ?1)",
                    [parent],
                    |row| row.get(0),
                )
                .map_err(storage_error)?;
            if !parent_exists {
                continue;
            }
            let highlight = Highlight {
                id: api.document.id.clone(),
                text: text.to_owned(),
                offset: api.highlight_offset,
                chapter: None,
            };
            let existing: Option<(String, i64)> = transaction
                .query_row(
                    "SELECT data, local_mutation FROM highlights WHERE id = ?1",
                    [&highlight.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(storage_error)?;
            let (highlight, mutation) = if let Some((data, mutation)) = existing {
                let local: Highlight = parse(&data)?;
                if mutation > started {
                    (local, mutation)
                } else {
                    (
                        Highlight {
                            offset: if local.chapter.is_some() {
                                local.offset.or(highlight.offset)
                            } else {
                                highlight.offset.or(local.offset)
                            },
                            chapter: local.chapter,
                            ..highlight
                        },
                        0,
                    )
                }
            } else {
                (highlight, 0)
            };
            transaction.execute(
                "INSERT INTO highlights(id, parent_id, data, local_mutation) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET parent_id = excluded.parent_id, data = excluded.data, local_mutation = excluded.local_mutation",
                params![highlight.id, parent, json(&highlight)?, mutation],
            ).map_err(storage_error)?;
        }
        if full {
            transaction.execute(
                "DELETE FROM documents WHERE local_mutation <= ?1 AND id NOT IN (SELECT id FROM sync_ids)",
                [started],
            ).map_err(storage_error)?;
            transaction.execute(
                "DELETE FROM highlights WHERE local_mutation <= ?1 AND id NOT IN (SELECT id FROM sync_ids)",
                [started],
            ).map_err(storage_error)?;
        }
        if advance_cursor {
            transaction
                .execute(
                    "INSERT INTO sync_state(key, value) VALUES ('last_synced', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [started_at],
                )
                .map_err(storage_error)?;
        }
        transaction
            .execute("DELETE FROM sync_ids", [])
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)
    }

    pub fn set_archived(&self, id: &str) -> Result<(), String> {
        let mut document = self.document(id)?;
        document.location = "archive".into();
        self.connection
            .execute(
                "UPDATE documents SET data = ?2, local_mutation = ?3 WHERE id = ?1",
                params![id, json(&document)?, chrono::Utc::now().timestamp_micros()],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn document(&self, id: &str) -> Result<Document, String> {
        let data: Option<String> = self
            .connection
            .query_row("SELECT data FROM documents WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage_error)?;
        parse(&data.ok_or_else(|| {
            "This document is no longer in the library. Refresh the library.".to_string()
        })?)
    }

    pub fn content(&self, id: &str) -> Result<Option<String>, String> {
        let content = self
            .connection
            .query_row("SELECT html FROM content WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage_error)?;
        if content.is_some() {
            self.connection
                .execute(
                    "UPDATE content SET accessed = ?2 WHERE id = ?1",
                    params![id, chrono::Utc::now().timestamp_micros()],
                )
                .map_err(storage_error)?;
        }
        Ok(content)
    }

    pub fn save_content(&mut self, id: &str, html: &str) -> Result<(), String> {
        if html.len() > CACHE_BYTES {
            return Err(
                "This document is too large for the local cache. Open it in Reader.".into(),
            );
        }
        let transaction = self.connection.transaction().map_err(storage_error)?;
        transaction.execute(
            "INSERT INTO content(id, html, bytes, accessed) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET html = excluded.html, bytes = excluded.bytes, accessed = excluded.accessed",
            params![id, html, html.len() as i64, chrono::Utc::now().timestamp_micros()],
        ).map_err(storage_error)?;
        let mut bytes: i64 = transaction
            .query_row("SELECT COALESCE(SUM(bytes), 0) FROM content", [], |row| {
                row.get(0)
            })
            .map_err(storage_error)?;
        while bytes > CACHE_BYTES as i64 {
            let (oldest, size): (String, i64) = transaction
                .query_row(
                    "SELECT id, bytes FROM content ORDER BY accessed, id LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(storage_error)?;
            transaction
                .execute("DELETE FROM content WHERE id = ?1", [oldest])
                .map_err(storage_error)?;
            bytes -= size;
        }
        transaction.commit().map_err(storage_error)?;
        self.connection
            .execute_batch("PRAGMA incremental_vacuum(256);")
            .map_err(storage_error)
    }

    pub fn position(&self, id: &str) -> Result<Option<Position>, String> {
        let data: Option<String> = self
            .connection
            .query_row("SELECT data FROM positions WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(storage_error)?;
        data.map(|data| parse(&data)).transpose()
    }

    pub fn save_position(&self, id: &str, position: &Position) -> Result<(), String> {
        if position.anchor.len() > 512
            || !position.progress.is_finite()
            || !(0.0..=1.0).contains(&position.progress)
            || !position.reader_progress.is_finite()
            || !(0.0..=1.0).contains(&position.reader_progress)
        {
            return Err("The reading position is invalid.".into());
        }
        self.connection
            .execute(
                "INSERT INTO positions(id, data) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data",
                params![id, json(position)?],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn highlights(&self, id: &str) -> Result<Vec<Highlight>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT data FROM highlights WHERE parent_id = ?1 ORDER BY id")
            .map_err(storage_error)?;
        let rows = statement
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        rows.map(|row| parse(&row.map_err(storage_error)?))
            .collect()
    }

    pub fn save_highlight(
        &self,
        parent: &str,
        id: &str,
        text: &str,
        offset: Option<u64>,
        chapter: Option<usize>,
    ) -> Result<(), String> {
        let highlight = Highlight {
            id: id.into(),
            text: text.into(),
            offset,
            chapter,
        };
        self.connection.execute(
            "INSERT INTO highlights(id, parent_id, data, local_mutation) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET parent_id = excluded.parent_id, data = excluded.data, local_mutation = excluded.local_mutation",
            params![id, parent, json(&highlight)?, chrono::Utc::now().timestamp_micros()],
        ).map_err(storage_error)?;
        Ok(())
    }

    pub fn settings(&self) -> Result<Settings, String> {
        let data: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM preferences WHERE key = 'settings'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)?;
        let settings: Settings = match data {
            Some(data) => parse(&data)?,
            None => Settings::default(),
        };
        settings.validate()?;
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings.validate()?;
        self.connection
            .execute(
                "INSERT INTO preferences(key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json(settings)?],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn clear_account(&mut self) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(storage_error)?;
        transaction
            .execute("DELETE FROM documents", [])
            .map_err(storage_error)?;
        transaction
            .execute("DELETE FROM sync_state", [])
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)?;
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA incremental_vacuum;")
            .map_err(storage_error)
    }
}

fn storage_error(error: rusqlite::Error) -> String {
    format!("The local reader database could not complete this operation ({error}).")
}

fn parse<T: DeserializeOwned>(data: &str) -> Result<T, String> {
    serde_json::from_str(data).map_err(|_| {
        "The local reader data is unreadable. Restore the database or reconnect.".into()
    })
}

fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|_| "Could not encode the local reader data.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIRST_SYNC: &str = "2000-01-01T00:00:00Z";
    const LATER_SYNC: &str = "2000-01-02T00:00:00Z";

    fn article(id: &str) -> ApiDocument {
        serde_json::from_value(serde_json::json!({
            "id": id, "url": format!("https://read.readwise.io/read/{id}"),
            "title": format!("Article {id}"), "category": "article", "location": "new",
            "updated_at": FIRST_SYNC, "tags": {}, "reading_time": "10 mins"
        }))
        .unwrap()
    }

    fn highlight(id: &str, parent: &str) -> ApiDocument {
        serde_json::from_value(serde_json::json!({
            "id": id, "title": "Metadata title", "location": "new", "content": "A highlighted passage", "category": "highlight",
            "parent_id": parent, "highlight_offset": 42, "notes": "A note"
        }))
        .unwrap()
    }

    #[test]
    fn partial_and_incremental_sync_preserve_unseen_items_until_completed_full_refresh() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("library.sqlite")).unwrap();
        let docs = vec![article("a"), article("b"), highlight("h", "b")];
        store.apply_sync(&docs, FIRST_SYNC, true).unwrap();
        store.save_content("b", "<p>Old article body</p>").unwrap();
        let position = Position {
            chapter: 0,
            anchor: "Old article".into(),
            offset: 12,
            progress: 0.2,
            reader_progress: 0.1,
        };
        store.save_position("b", &position).unwrap();
        assert_eq!(store.library().unwrap().documents.len(), 2);
        assert_eq!(store.highlights("b").unwrap()[0].offset, Some(42));
        assert_eq!(
            store.highlights("b").unwrap()[0].text,
            "A highlighted passage"
        );

        let mut empty = highlight("h", "b");
        empty.content = Some(String::new());
        let mut missing = highlight("no-passage", "b");
        missing.content = None;
        store
            .apply_sync(
                &[article("a"), article("b"), empty, missing],
                FIRST_SYNC,
                true,
            )
            .unwrap();
        let retained = store.highlights("b").unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].text, "A highlighted passage");

        let mut repaired = article("b");
        repaired.document.updated_at = LATER_SYNC.into();
        store.upsert_partial(&[repaired], LATER_SYNC).unwrap();
        assert!(store.content("b").unwrap().is_none());
        assert_eq!(store.position("b").unwrap().unwrap().offset, 12);

        store.upsert_partial(&[article("c")], LATER_SYNC).unwrap();
        assert_eq!(store.library().unwrap().documents.len(), 3);
        assert_eq!(
            store.library().unwrap().last_synced.as_deref(),
            Some(FIRST_SYNC)
        );
        store
            .apply_sync(&[article("a")], LATER_SYNC, false)
            .unwrap();
        assert_eq!(store.library().unwrap().documents.len(), 3);
        assert_eq!(
            store.library().unwrap().last_synced.as_deref(),
            Some(LATER_SYNC)
        );

        store
            .apply_sync(&[article("a")], "2000-01-03T00:00:00Z", true)
            .unwrap();
        assert_eq!(store.library().unwrap().documents.len(), 1);
        assert!(store.document("b").is_err());
        assert!(store.content("b").unwrap().is_none());
        assert!(store.position("b").unwrap().is_none());
        assert!(store.highlights("b").unwrap().is_empty());
        assert!(store.apply_sync(&[], "invalid timestamp", true).is_err());
        assert_eq!(store.library().unwrap().documents.len(), 1);
        assert_eq!(
            store.library().unwrap().last_synced.as_deref(),
            Some("2000-01-03T00:00:00Z")
        );
    }

    #[test]
    fn older_sync_cannot_undo_newer_successful_archive_or_highlight() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("library.sqlite")).unwrap();
        store.apply_sync(&[article("a")], FIRST_SYNC, true).unwrap();
        store.set_archived("a").unwrap();
        store
            .save_highlight("a", "local-h", "Selected text", Some(13), Some(2))
            .unwrap();
        store.upsert_partial(&[article("a")], LATER_SYNC).unwrap();
        assert_eq!(store.document("a").unwrap().location, "archive");
        store
            .upsert_partial(&[highlight("local-h", "a")], LATER_SYNC)
            .unwrap();
        assert_eq!(store.highlights("a").unwrap()[0].text, "Selected text");
        store.apply_sync(&[], LATER_SYNC, true).unwrap();
        assert_eq!(store.document("a").unwrap().location, "archive");
        let highlights = store.highlights("a").unwrap();
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].text, "Selected text");
        assert_eq!(highlights[0].chapter, Some(2));

        store
            .apply_sync(
                &[article("a"), highlight("local-h", "a")],
                "2100-01-01T00:00:00Z",
                true,
            )
            .unwrap();
        assert_eq!(store.document("a").unwrap().location, "new");
        let highlight = &store.highlights("a").unwrap()[0];
        assert_eq!(highlight.offset, Some(13));
        assert_eq!(highlight.chapter, Some(2));
    }

    #[test]
    fn settings_and_positions_survive_restart_and_account_clear_isolates_library_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite");
        {
            let store = Store::open(&path).unwrap();
            // This is the persisted v0.1 format, before the additional reading controls existed.
            store.connection.execute(
                "INSERT INTO preferences(key, value) VALUES ('settings', ?1)",
                [r#"{"font_size":26,"line_height":1.6,"reading_width":1100,"view":"list","sort":"oldest"}"#],
            ).unwrap();
            let upgraded = store.settings().unwrap();
            assert_eq!(upgraded.font_size, 26);
            assert_eq!(upgraded.reading_width, 1100);
            assert_eq!(upgraded.font_family, "inter");
            assert_eq!(upgraded.cover_size, 280);
            assert_eq!(upgraded.text_brightness, 83);
        }
        let settings = Settings {
            font_size: 24,
            line_height: 1.6,
            reading_width: 1200,
            font_family: "georgia".into(),
            font_weight: 500,
            paragraph_spacing: 1.2,
            text_brightness: 90,
            cover_size: 420,
            view: "list".into(),
            sort: "oldest".into(),
        };
        let position = Position {
            chapter: 3,
            anchor: "An exact passage".into(),
            offset: 22,
            progress: 0.3,
            reader_progress: 0.25,
        };
        {
            let mut store = Store::open(&path).unwrap();
            store
                .apply_sync(&[article("private-a")], FIRST_SYNC, true)
                .unwrap();
            store.save_settings(&settings).unwrap();
            store.save_position("private-a", &position).unwrap();
            store
                .save_content("private-a", "<p>Private account content</p>")
                .unwrap();
            store
                .save_highlight("private-a", "private-h", "Private highlight", Some(3), None)
                .unwrap();
            let invalid = Settings {
                font_size: 100,
                ..settings.clone()
            };
            assert!(store.save_settings(&invalid).is_err());
            for invalid in [
                Settings {
                    font_family: "untrusted-font".into(),
                    ..settings.clone()
                },
                Settings {
                    font_weight: 900,
                    ..settings.clone()
                },
                Settings {
                    paragraph_spacing: f64::NAN,
                    ..settings.clone()
                },
                Settings {
                    text_brightness: 20,
                    ..settings.clone()
                },
                Settings {
                    cover_size: 100,
                    ..settings.clone()
                },
            ] {
                assert!(store.save_settings(&invalid).is_err());
            }
        }
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.settings().unwrap().font_size, 24);
        let restored = store.settings().unwrap();
        assert_eq!(restored.font_family, "georgia");
        assert_eq!(restored.font_weight, 500);
        assert_eq!(restored.paragraph_spacing, 1.2);
        assert_eq!(restored.text_brightness, 90);
        assert_eq!(restored.cover_size, 420);
        assert_eq!(store.position("private-a").unwrap().unwrap().chapter, 3);
        assert_eq!(
            store.position("private-a").unwrap().unwrap().anchor,
            "An exact passage"
        );
        assert_eq!(
            store.content("private-a").unwrap().as_deref(),
            Some("<p>Private account content</p>")
        );
        store.clear_account().unwrap();
        assert!(store.library().unwrap().documents.is_empty());
        assert!(store.library().unwrap().last_synced.is_none());
        assert!(store.content("private-a").unwrap().is_none());
        assert!(store.position("private-a").unwrap().is_none());
        assert!(store.highlights("private-a").unwrap().is_empty());
        assert_eq!(store.settings().unwrap().reading_width, 1200);
        store
            .apply_sync(&[article("other-account")], LATER_SYNC, true)
            .unwrap();
        assert!(store.document("private-a").is_err());
        assert_eq!(store.library().unwrap().documents[0].id, "other-account");
    }

    #[test]
    fn body_cache_evicts_least_recently_read_content_without_losing_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("library.sqlite")).unwrap();
        let ids = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
        let docs: Vec<_> = ids.iter().map(|id| article(id)).collect();
        store.apply_sync(&docs, FIRST_SYNC, true).unwrap();
        // Nine API-sized bodies exceed 128 MiB; reading a keeps it ahead of b.
        let body = "x".repeat(15 * 1024 * 1024);
        for id in &ids[..8] {
            store.save_content(id, &body).unwrap();
        }
        assert!(store.content("a").unwrap().is_some());
        store.save_content("i", &body).unwrap();
        assert!(store.content("a").unwrap().is_some());
        assert!(store.content("b").unwrap().is_none());
        assert!(store.content("i").unwrap().is_some());
        assert_eq!(store.library().unwrap().documents.len(), 9);
    }
}
