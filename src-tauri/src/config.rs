use crate::models::{CustomView, ViewsConfig};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    views: Vec<CustomView>,
}

pub fn load(path: &Path) -> ViewsConfig {
    let read = || -> Result<Vec<CustomView>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let size = fs::metadata(path)
            .map_err(|e| format!("Cannot read views config: {e}"))?
            .len();
        if size > 64 * 1024 {
            return Err("Views config is larger than 64 KB.".into());
        }
        let source =
            fs::read_to_string(path).map_err(|e| format!("Cannot read views config: {e}"))?;
        let config: ConfigFile =
            serde_json::from_str(&source).map_err(|e| format!("Invalid views.json: {e}"))?;
        if config.views.len() > 30 {
            return Err("Use at most 30 custom views.".into());
        }
        let mut names = std::collections::HashSet::new();
        for view in &config.views {
            if view.name.trim().is_empty()
                || view.name.len() > 80
                || view.query.len() > 2048
                || view.query.trim().is_empty()
            {
                return Err(
                    "Each view needs a name (1–80 characters) and a query (1–2048 characters)."
                        .into(),
                );
            }
            if !names.insert(view.name.to_lowercase())
                || ["inbox", "later", "books"].contains(&view.name.to_lowercase().as_str())
            {
                return Err(format!(
                    "View name '{}' is duplicated or reserved.",
                    view.name
                ));
            }
        }
        Ok(config.views)
    };
    match read() {
        Ok(views) => ViewsConfig {
            views,
            config_path: path.display().to_string(),
            config_error: None,
        },
        Err(error) => ViewsConfig {
            views: Vec::new(),
            config_path: path.display().to_string(),
            config_error: Some(error),
        },
    }
}

pub fn open(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::write(path, "{\n  \"views\": []\n}\n")
            .map_err(|e| format!("Cannot create views config: {e}"))?;
    }
    std::process::Command::new("notepad.exe")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Cannot open the config in Notepad: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_config_loads_views_and_reports_bad_input_without_hiding_builtins() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("views.json");
        let absent = load(&path);
        assert!(absent.views.is_empty());
        assert!(absent.config_error.is_none());
        fs::write(&path, r#"{"views":[{"name":"Essays","query":"in:later"}]}"#).unwrap();
        let valid = load(&path);
        assert_eq!(valid.views[0].name, "Essays");
        assert_eq!(valid.views[0].query, "in:later");
        for source in [
            r#"{"views":[{"name":"Inbox","query":"in:later"}]}"#,
            r#"{"views":[],"typo":true}"#,
            "invalid JSON",
        ] {
            fs::write(&path, source).unwrap();
            let invalid = load(&path);
            assert!(invalid.views.is_empty());
            assert!(invalid.config_error.is_some());
        }
    }
}
