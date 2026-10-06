use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Document {
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default, deserialize_with = "nullable_string")]
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub site_name: Option<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default, deserialize_with = "nullable_string")]
    pub location: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub word_count: Option<u64>,
    #[serde(default)]
    pub reading_time: Option<String>,
    #[serde(default, deserialize_with = "nullable_progress")]
    pub reading_progress: f64,
    #[serde(default)]
    pub saved_at: Option<String>,
    #[serde(default)]
    pub published_date: Option<String>,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default, deserialize_with = "nullable_tags")]
    pub tags: HashMap<String, serde_json::Value>,
}

// Reader's highlight documents have no title or library location.
fn nullable_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn nullable_progress<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(deserializer)?
        .unwrap_or(0.0)
        .clamp(0.0, 1.0))
}

fn nullable_tags<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<String, serde_json::Value>, D::Error> {
    Ok(
        Option::<HashMap<String, serde_json::Value>>::deserialize(deserializer)?
            .unwrap_or_default(),
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiDocument {
    #[serde(flatten)]
    pub document: Document,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub html_content: Option<String>,
    #[serde(default)]
    pub raw_source_url: Option<String>,
    #[serde(default)]
    pub highlight_offset: Option<u64>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub font_size: u32,
    pub line_height: f64,
    pub reading_width: u32,
    pub font_family: String,
    pub font_weight: u32,
    pub paragraph_spacing: f64,
    pub text_brightness: u32,
    pub cover_size: u32,
    pub view: String,
    pub sort: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "black".into(),
            font_size: 20,
            line_height: 1.8,
            reading_width: 900,
            font_family: "inter".into(),
            font_weight: 400,
            paragraph_spacing: 1.5,
            text_brightness: 83,
            cover_size: 280,
            view: "covers".into(),
            sort: "newest".into(),
        }
    }
}

impl Settings {
    pub const MIN_COVER_SIZE: u32 = 140;
    pub const MAX_COVER_SIZE: u32 = 360;

    pub fn validate(&self) -> Result<(), String> {
        if !["black", "dark", "light"].contains(&self.theme.as_str())
            || !(16..=32).contains(&self.font_size)
            || !(1.4..=2.2).contains(&self.line_height)
            || !(600..=1400).contains(&self.reading_width)
            || !["inter", "georgia", "system"].contains(&self.font_family.as_str())
            || !(300..=700).contains(&self.font_weight)
            || !(0.5..=2.5).contains(&self.paragraph_spacing)
            || !(60..=100).contains(&self.text_brightness)
            || !(Self::MIN_COVER_SIZE..=Self::MAX_COVER_SIZE).contains(&self.cover_size)
            || !["covers", "list"].contains(&self.view.as_str())
            || !["newest", "oldest", "shortest"].contains(&self.sort.as_str())
        {
            return Err("Reading settings are outside the supported range.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Position {
    pub chapter: usize,
    pub anchor: String,
    pub offset: usize,
    pub progress: f64,
    pub reader_progress: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Highlight {
    pub id: String,
    pub text: String,
    pub offset: Option<u64>,
    pub chapter: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Chapter {
    pub index: usize,
    pub title: String,
}

#[derive(Serialize)]
pub struct ReadingDocument {
    pub document: Document,
    pub html: String,
    pub highlights: Vec<Highlight>,
    pub position: Option<Position>,
    pub chapters: Vec<Chapter>,
    pub chapter: usize,
}

#[derive(Serialize)]
pub struct LibrarySnapshot {
    pub documents: Vec<Document>,
    pub last_synced: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CustomView {
    pub name: String,
    pub query: String,
}

#[derive(Serialize)]
pub struct ViewsConfig {
    pub views: Vec<CustomView>,
    pub config_path: String,
    pub config_error: Option<String>,
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub connected: bool,
    pub documents: Vec<Document>,
    pub last_synced: Option<String>,
    pub settings: Settings,
    pub views: Vec<CustomView>,
    pub config_path: String,
    pub config_error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct SyncProgress {
    pub message: String,
    pub completed: usize,
}
