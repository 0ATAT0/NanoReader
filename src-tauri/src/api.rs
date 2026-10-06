use crate::models::ApiDocument;
use reqwest::{header, Client, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

const LIST_INTERVAL: Duration = Duration::from_millis(3100);
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_RETRY_WAIT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
pub struct Page {
    // The final page has a null cursor; omission is an unexpected response, not completion.
    #[serde(
        rename = "nextPageCursor",
        deserialize_with = "Deserialize::deserialize"
    )]
    pub next_page_cursor: Option<String>,
    pub results: Vec<ApiDocument>,
}

#[derive(Clone)]
pub struct ReaderApi {
    client: Client,
    next_list: Arc<Mutex<Instant>>,
    progress: Arc<dyn Fn(String) + Send + Sync>,
}

impl ReaderApi {
    pub fn new(token: String, progress: Arc<dyn Fn(String) + Send + Sync>) -> Result<Self, String> {
        if token.trim().is_empty() {
            return Err("Enter your Readwise access token.".into());
        }
        let mut authorization =
            header::HeaderValue::from_str(&format!("Token {}", token.trim()))
                .map_err(|_| "The access token contains invalid characters.".to_string())?;
        authorization.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, authorization);
        let client = Client::builder()
            .default_headers(headers)
            .user_agent(concat!("NanoReader/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(|_| "Could not initialize the secure Readwise connection.".to_string())?;
        Ok(Self {
            client,
            next_list: Arc::new(Mutex::new(Instant::now())),
            progress,
        })
    }

    pub async fn validate_token(&self) -> Result<(), String> {
        let response = self
            .send(self.client.get("https://readwise.io/api/v2/auth/"), false)
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(status_error(response.status(), "validate your token"))
        }
    }

    pub async fn list_page(&self, params: &[(&str, String)]) -> Result<Page, String> {
        let response = self
            .send(
                self.client
                    .get("https://readwise.io/api/v3/list/")
                    .query(params),
                true,
            )
            .await?;
        if response.status() != StatusCode::OK {
            return Err(status_error(response.status(), "load the library"));
        }
        decode(response).await
    }

    pub async fn archive(&self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        let response = self
            .send(
                self.client
                    .patch(format!("https://readwise.io/api/v3/update/{id}/"))
                    .json(&serde_json::json!({"location": "archive"})),
                false,
            )
            .await?;
        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(status_error(response.status(), "archive this document"))
        }
    }

    pub async fn highlight(&self, parent_id: &str, text: &str) -> Result<String, String> {
        validate_id(parent_id)?;
        if text.trim().is_empty() {
            return Err("Select a passage of text before creating a highlight.".into());
        }
        if text.len() > 20_000 {
            return Err("Select a shorter passage before creating a highlight.".into());
        }
        let response = self
            .send(
                self.client
                    .post("https://readwise.io/api/v3/save/")
                    .json(&serde_json::json!({
                        "parent_id": parent_id,
                        "content": text,
                        "saved_using": "NanoReader"
                    })),
                false,
            )
            .await?;
        if response.status() == StatusCode::BAD_REQUEST {
            return Err("Reader could not match this passage. Select text from the article, or highlight it in Reader.".into());
        }
        if !matches!(response.status(), StatusCode::OK | StatusCode::CREATED) {
            return Err(status_error(response.status(), "save the highlight"));
        }
        #[derive(Deserialize)]
        struct Created {
            id: String,
        }
        let created: Created = decode(response).await?;
        validate_id(&created.id)?;
        Ok(created.id)
    }

    async fn wait_for_list(&self) {
        // Clones and concurrent content requests share the same documented LIST budget.
        loop {
            let deadline = {
                let mut next = self.next_list.lock().await;
                let now = Instant::now();
                if *next <= now {
                    *next = now + LIST_INTERVAL;
                    return;
                }
                *next
            };
            (self.progress)("Waiting for Reader's library request limit…".into());
            // Release the lock so a 429 can extend the deadline while another caller waits.
            tokio::time::sleep_until(deadline).await;
        }
    }

    async fn send(&self, request: RequestBuilder, is_list: bool) -> Result<Response, String> {
        for attempt in 0..3 {
            if is_list {
                self.wait_for_list().await;
            }
            let response = request
                .try_clone()
                .ok_or_else(|| "Could not prepare the Readwise request.".to_string())?
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        "Readwise timed out. Check your connection and try again.".to_string()
                    } else {
                        "Could not reach Readwise securely. Check your connection and try again."
                            .to_string()
                    }
                })?;
            if response.status() != StatusCode::TOO_MANY_REQUESTS {
                return Ok(response);
            }
            let wait = retry_after(response.headers());
            if attempt == 2 || wait > MAX_RETRY_WAIT {
                return Err(
                    "Readwise is rate limiting requests. Wait a few minutes and try again.".into(),
                );
            }
            if is_list {
                let mut next = self.next_list.lock().await;
                *next = (*next).max(Instant::now() + wait);
            }
            (self.progress)(format!(
                "Reader request limit reached; retrying in {} seconds…",
                wait.as_secs()
            ));
            tokio::time::sleep(wait).await;
        }
        unreachable!()
    }
}

fn retry_after(headers: &header::HeaderMap) -> Duration {
    let Some(value) = headers
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
    else {
        return Duration::from_secs(60);
    };
    if let Ok(seconds) = value.parse::<u64>() {
        return Duration::from_secs(seconds.max(1));
    }
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(value) {
        let seconds = (date.timestamp() - chrono::Utc::now().timestamp()).max(1) as u64;
        return Duration::from_secs(seconds);
    }
    Duration::from_secs(60)
}

fn status_error(status: StatusCode, action: &str) -> String {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            "Readwise rejected the access token. Reconnect with a valid token.".into()
        }
        StatusCode::NOT_FOUND => {
            "The document is no longer available in Reader. Refresh the library.".into()
        }
        StatusCode::BAD_REQUEST => {
            format!("Readwise rejected the request to {action}. Refresh and try again.")
        }
        status if status.is_server_error() => {
            "Readwise is temporarily unavailable. Try again shortly.".into()
        }
        status => format!(
            "Could not {action}: Readwise returned HTTP {}.",
            status.as_u16()
        ),
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Reader returned an invalid document identifier. Refresh the library.".into());
    }
    Ok(())
}

async fn decode<T: serde::de::DeserializeOwned>(mut response: Response) -> Result<T, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "The Readwise response was interrupted. Try again.".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(
                "The Readwise response is too large to load safely. Open this item in Reader."
                    .into(),
            );
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        // The full serde error can contain private response values.
        format!(
            "Readwise returned an unexpected response format ({:?}, line {}, column {}). Update the app or try again later.",
            error.classify(), error.line(), error.column()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_pages_decode_nullable_metadata_and_cursor_contract() {
        let page: Page = serde_json::from_str(
            r#"{
                "count": 3, "nextPageCursor": "next-library-page",
                "results": [
                    {"id":"article-a","title":"First article","category":"article","location":"new",
                     "author":null,"source_url":null,"image_url":null,"word_count":null,
                     "reading_time":null,"reading_progress":null,"tags":null,"parent_id":null,
                     "html_content":null,"raw_source_url":null,"highlight_offset":null,"notes":null},
                    {"id":"highlight-a","title":null,"location":null,"category":"highlight",
                     "content":"A precise imported passage", "parent_id":"article-a",
                     "highlight_offset":25,"notes":"A note"}
                ]
            }"#,
        ).unwrap();
        assert_eq!(page.next_page_cursor.as_deref(), Some("next-library-page"));
        assert_eq!(page.results.len(), 2);
        assert_eq!(page.results[0].document.reading_progress, 0.0);
        assert!(page.results[0].document.tags.is_empty());
        assert!(page.results[0].document.reading_time.is_none());
        assert_eq!(page.results[1].parent_id.as_deref(), Some("article-a"));
        assert_eq!(page.results[1].highlight_offset, Some(25));
        assert_eq!(page.results[1].notes.as_deref(), Some("A note"));
        assert!(page.results[1].document.title.is_empty());
        assert!(page.results[1].document.location.is_empty());

        let last: Page =
            serde_json::from_str(r#"{"count":null,"nextPageCursor":null,"results":[]}"#).unwrap();
        assert!(last.next_page_cursor.is_none());
        assert!(last.results.is_empty());
        assert!(serde_json::from_str::<Page>(r#"{"results":[]}"#).is_err());
        assert!(serde_json::from_str::<Page>(r#"{"nextPageCursor":null}"#).is_err());
        assert!(serde_json::from_str::<Page>(
            r#"{"nextPageCursor":null,"results":[{"title":"missing id"}]}"#
        )
        .is_err());
        for field in ["title", "location"] {
            for value in [
                serde_json::json!(7),
                serde_json::json!({"unexpected":"object"}),
            ] {
                let wrong_type = serde_json::json!({
                    "nextPageCursor": null,
                    "results": [{"id":"invalid-metadata", field: value}]
                });
                assert!(serde_json::from_value::<Page>(wrong_type).is_err());
            }
        }
    }

    #[test]
    fn retry_after_accepts_delta_and_http_date_without_shortening_long_limits() {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, header::HeaderValue::from_static("17"));
        assert_eq!(retry_after(&headers), Duration::from_secs(17));
        headers.insert(
            header::RETRY_AFTER,
            header::HeaderValue::from_static("3600"),
        );
        assert_eq!(retry_after(&headers), Duration::from_secs(3600));
        headers.insert(
            header::RETRY_AFTER,
            header::HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        assert_eq!(retry_after(&headers), Duration::from_secs(1));
        headers.remove(header::RETRY_AFTER);
        assert_eq!(retry_after(&headers), Duration::from_secs(60));
    }

    #[test]
    fn transport_keeps_auth_redirect_policy_and_rate_cooldown_shared_across_clones() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::mpsc,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (first_tx, first_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let fixture = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            let mut limited = false;
            let mut rate_start = None;
            let mut arrivals = Vec::new();
            for _ in 0..5 {
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => {
                            panic!("HTTP fixture did not receive the expected request: {error}")
                        }
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                    let mut chunk = [0; 1024];
                    let count = socket.read(&mut chunk).unwrap();
                    assert!(
                        count > 0 && bytes.len() < 8192,
                        "Incomplete HTTP fixture headers"
                    );
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let headers = String::from_utf8(bytes).unwrap();
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let authorization = headers.lines().find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim())
                });
                assert_eq!(authorization, Some("Token fixture-token"));
                let response = if path == "/redirect" {
                    format!("HTTP/1.1 302 Found\r\nLocation: http://{address}/must-not-follow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                } else if path == "/malformed" {
                    let body = r#"{"nextPageCursor":null,"results":[{"id":"a","reading_progress":"PRIVATE-FIXTURE-PASSAGE"}]}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else if path == "/limited" && !limited {
                    limited = true;
                    first_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    rate_start = Some(std::time::Instant::now());
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 4\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
                } else {
                    assert!(
                        matches!(path, "/clone" | "/limited"),
                        "The client followed a redirect"
                    );
                    arrivals.push(rate_start.unwrap().elapsed());
                    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
                };
                socket.write_all(response.as_bytes()).unwrap();
            }
            arrivals
        });
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (progress_tx, progress_rx) = mpsc::channel();
            let api = ReaderApi::new(
                "fixture-token".into(),
                Arc::new(move |message| {
                    let _ = progress_tx.send(message);
                }),
            )
            .unwrap();
            let redirect = api
                .send(api.client.get(format!("http://{address}/redirect")), false)
                .await
                .unwrap();
            assert_eq!(redirect.status(), StatusCode::FOUND);
            let malformed = api
                .send(api.client.get(format!("http://{address}/malformed")), false)
                .await
                .unwrap();
            let error = decode::<Page>(malformed).await.unwrap_err();
            assert!(error.contains("Data, line 1, column "));
            assert!(!error.contains("PRIVATE-FIXTURE-PASSAGE"));
            let limited = tokio::spawn({
                let api = api.clone();
                async move {
                    api.send(api.client.get(format!("http://{address}/limited")), true)
                        .await
                }
            });
            first_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let cloned = tokio::spawn({
                let api = api.clone();
                async move {
                    api.send(api.client.get(format!("http://{address}/clone")), true)
                        .await
                }
            });
            // The second request is already waiting when the server extends the shared limit.
            progress_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            release_tx.send(()).unwrap();
            assert_eq!(cloned.await.unwrap().unwrap().status(), StatusCode::OK);
            assert_eq!(limited.await.unwrap().unwrap().status(), StatusCode::OK);
        });
        let arrivals = fixture.join().unwrap();
        assert_eq!(arrivals.len(), 2);
        assert!(arrivals
            .iter()
            .all(|elapsed| *elapsed >= Duration::from_millis(3800)));
        assert!(arrivals[1] - arrivals[0] >= Duration::from_secs(3));
    }
}
