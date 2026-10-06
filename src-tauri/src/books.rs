use crate::models::Chapter;
use base64::{engine::general_purpose::STANDARD, Engine};
use epub::doc::{EpubDoc, NavPoint};
use quick_xml::{
    events::{BytesStart, Event},
    Reader, Writer,
};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

const CACHE_LIMIT: u64 = 128 * 1024 * 1024;
const IMAGE_LIMIT: usize = 8 * 1024 * 1024;

pub fn path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("The book identifier is invalid.".into());
    }
    Ok(root.join("books").join(format!("{id}.epub")))
}

pub fn cached(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::metadata(path).map_err(|e| format!("Cannot read cached book: {e}"))?;
    if metadata.len() > CACHE_LIMIT {
        return Err("This book exceeds the 128 MB reading limit. Open it in Reader.".into());
    }
    fs::read(path)
        .map(Some)
        .map_err(|e| format!("Cannot read cached book: {e}"))
}

pub fn save(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let folder = path.parent().ok_or("Invalid book cache path.")?;
    fs::create_dir_all(folder).map_err(|e| format!("Cannot create book cache: {e}"))?;
    let mut entries = fs::read_dir(folder)
        .map_err(|e| format!("Cannot read book cache: {e}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok().map(|meta| (entry.path(), meta)))
        .filter(|(_, meta)| meta.is_file())
        .collect::<Vec<_>>();
    entries.sort_by_key(|(_, meta)| meta.modified().ok());
    let mut total: u64 = entries.iter().map(|(_, meta)| meta.len()).sum();
    for (entry, meta) in entries {
        if total + bytes.len() as u64 <= CACHE_LIMIT {
            break;
        }
        fs::remove_file(&entry).map_err(|e| format!("Cannot trim book cache: {e}"))?;
        total -= meta.len();
    }
    let temp = path.with_extension("part");
    fs::write(&temp, bytes).map_err(|e| format!("Cannot cache book: {e}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Cannot replace cached book: {e}"))?;
    }
    fs::rename(temp, path).map_err(|e| format!("Cannot finish book cache write: {e}"))
}

pub fn clear(root: &Path) -> Result<(), String> {
    let folder = root.join("books");
    if folder.exists() {
        fs::remove_dir_all(folder).map_err(|e| format!("Cannot clear book cache: {e}"))?;
    }
    Ok(())
}

pub async fn download(source: &str) -> Result<Vec<u8>, String> {
    let url =
        url::Url::parse(source).map_err(|_| "Reader returned an invalid book-source link.")?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|host| host.ends_with(".amazonaws.com"))
        || !url.username().is_empty()
    {
        return Err("Reader returned an unsupported book-source host.".into());
    }
    // The signed URL carries its own credentials; never send the Readwise token here.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|_| "Cannot start the book download.")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "The book download failed. Check your connection and retry.")?;
    if !response.status().is_success() {
        return Err(
            "The book-source link expired or could not be downloaded. Retry or open in Reader."
                .into(),
        );
    }
    if response
        .content_length()
        .is_some_and(|length| length > CACHE_LIMIT)
    {
        return Err("This book is larger than 128 MB. Open it in Reader.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "The book download was interrupted.")?
    {
        if bytes.len() + chunk.len() > CACHE_LIMIT as usize {
            return Err("This book is larger than 128 MB. Open it in Reader.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub fn read(
    bytes: Vec<u8>,
    chosen: Option<usize>,
    progress: f64,
) -> Result<(String, Vec<Chapter>, usize), String> {
    let mut book = EpubDoc::from_reader(Cursor::new(bytes)).map_err(|_| {
        "This EPUB could not be parsed. It may be encrypted or damaged; open it in Reader."
    })?;
    if book.spine.is_empty() {
        return Err("This EPUB has no readable chapters.".into());
    }
    let chapters = book
        .spine
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let title = book
                .resources
                .get(&item.idref)
                .and_then(|resource| toc_title(&book.toc, &resource.path))
                .unwrap_or_else(|| format!("Chapter {}", index + 1));
            Chapter { index, title }
        })
        .collect::<Vec<_>>();
    let chapter = chosen.unwrap_or_else(|| {
        (progress.clamp(0.0, 1.0) * (chapters.len() - 1) as f64).round() as usize
    });
    if !book.set_current_chapter(chapter) {
        return Err("This chapter is no longer available. Open the book again.".into());
    }
    let source = book
        .get_current_with_epub_uris()
        .map_err(|_| "This EPUB chapter could not be read. Open it in Reader.")?;
    if source.len() > 32 * 1024 * 1024 {
        return Err("This chapter is too large to render. Open it in Reader.".into());
    }
    let current_path = book
        .get_current_path()
        .ok_or("This chapter has no content path.")?;
    let mut reader = Reader::from_reader(source.as_slice());
    let mut writer = Writer::new(Vec::new());
    let mut image_bytes = 0;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| "This EPUB chapter contains invalid markup.")?;
        match event {
            Event::Eof => break,
            Event::Start(start) => writer.write_event(Event::Start(rewrite(
                &start,
                &mut book,
                &current_path,
                &mut image_bytes,
            )?)),
            Event::Empty(start) => writer.write_event(Event::Empty(rewrite(
                &start,
                &mut book,
                &current_path,
                &mut image_bytes,
            )?)),
            other => writer.write_event(other.into_owned()),
        }
        .map_err(|_| "This EPUB chapter could not be rendered.")?;
    }
    let html = String::from_utf8(writer.into_inner())
        .map_err(|_| "This EPUB uses an unsupported text encoding.")?;
    Ok((html, chapters, chapter))
}

fn toc_title(points: &[NavPoint], path: &Path) -> Option<String> {
    for point in points {
        let without_fragment = point
            .content
            .to_string_lossy()
            .split('#')
            .next()
            .unwrap_or_default()
            .to_string();
        if Path::new(&without_fragment) == path {
            return Some(point.label.clone());
        }
        if let Some(label) = toc_title(&point.children, path) {
            return Some(label);
        }
    }
    None
}

fn rewrite(
    start: &BytesStart<'_>,
    book: &mut EpubDoc<Cursor<Vec<u8>>>,
    current_path: &Path,
    image_bytes: &mut usize,
) -> Result<BytesStart<'static>, String> {
    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut out = BytesStart::new(name.clone());
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|_| "This EPUB contains invalid attributes.")?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let mut value = attribute
            .unescape_value()
            .map_err(|_| "This EPUB contains an invalid resource link.")?
            .into_owned();
        if let Some(resource) = value.strip_prefix("epub://") {
            let (resource_path, fragment) = resource.split_once('#').unwrap_or((resource, ""));
            if name == "img" && key == "src" {
                value = if let Some(mime) = book.get_resource_mime_by_path(resource_path) {
                    if [
                        "image/png",
                        "image/jpeg",
                        "image/gif",
                        "image/webp",
                        "image/avif",
                    ]
                    .contains(&mime.as_str())
                    {
                        if let Some(image) = book.get_resource_by_path(resource_path) {
                            if image.len() <= IMAGE_LIMIT
                                && *image_bytes + image.len() <= 32 * 1024 * 1024
                            {
                                *image_bytes += image.len();
                                format!("data:{mime};base64,{}", STANDARD.encode(image))
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
            } else if name == "a" && key == "href" {
                if Path::new(resource_path) == current_path {
                    value = format!("#{fragment}");
                } else if let Some(index) = book.spine.iter().position(|item| {
                    book.resources
                        .get(&item.idref)
                        .is_some_and(|item| item.path == Path::new(resource_path))
                }) {
                    value = format!(
                        "#quiet-reader-chapter={index}&fragment={}",
                        url::form_urlencoded::byte_serialize(fragment.as_bytes())
                            .collect::<String>()
                    );
                } else {
                    value.clear();
                }
            } else {
                // Chapter navigation owns spine links; keep unsupported embedded resources inert.
                value.clear();
            }
        }
        out.push_attribute((key.as_str(), value.as_str()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_epub_renders_images_and_cross_chapter_references() {
        let bytes = include_bytes!("../fixtures/small-book.epub").to_vec();
        let (html, chapters, chapter) = read(bytes.clone(), Some(0), 0.8).unwrap();
        assert_eq!(chapter, 0);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "First observation");
        assert!(html.contains("First observation."));
        assert!(html.contains("data:image/png;base64,"));
        assert!(
            html.contains("#quiet-reader-chapter=1&amp;fragment=note")
                || html.contains("#quiet-reader-chapter=1&fragment=note")
        );
        let (html, _, chapter) = read(bytes.clone(), None, 0.8).unwrap();
        assert_eq!(chapter, 1);
        assert!(html.contains("Second observation."));
        assert!(read(bytes, Some(2), 0.0).is_err());
        assert!(read(vec![0, 1, 2], None, 0.0).is_err());
    }

    #[test]
    fn book_cache_survives_reopen_and_account_clear_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        assert!(path(directory.path(), "../other").is_err());
        let target = path(directory.path(), "book-1").unwrap();
        assert!(cached(&target).unwrap().is_none());
        save(&target, include_bytes!("../fixtures/small-book.epub")).unwrap();
        assert!(cached(&target).unwrap().is_some());
        clear(directory.path()).unwrap();
        assert!(cached(&target).unwrap().is_none());
    }
}
