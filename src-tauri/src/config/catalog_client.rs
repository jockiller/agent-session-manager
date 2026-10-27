use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use super::display_name;
use super::endpoint;
use super::models::{APIKind, ModelEntry, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    EmptyUrl,
    Http(u16),
    NoModels,
    Decode(String),
}

impl CatalogError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyUrl => "没有目录地址".to_string(),
            Self::Http(code) if *code == 401 || *code == 403 => {
                format!("目录认证失败（HTTP {code}），检查 API Key")
            }
            Self::Http(code) => format!("目录请求失败（HTTP {code}）"),
            Self::NoModels => "目录里没有模型".to_string(),
            Self::Decode(detail) => format!("目录解析失败：{detail}"),
        }
    }
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for CatalogError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEndpoints {
    pub management: String,
    pub v1: String,
}

impl CatalogEndpoints {
    pub fn split(raw: &str) -> Result<Self, CatalogError> {
        let mut text = endpoint::strip_any_endpoint(raw);
        while text.ends_with('/') {
            text.pop();
        }
        if text.is_empty() {
            return Err(CatalogError::EmptyUrl);
        }
        if let Some(management) = text.strip_suffix("/v1") {
            if management.is_empty() {
                return Err(CatalogError::EmptyUrl);
            }
            return Ok(Self {
                management: management.to_string(),
                v1: text,
            });
        }
        Ok(Self {
            management: text.clone(),
            v1: format!("{text}/v1"),
        })
    }
}

fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn build_client() -> Result<reqwest::Client, CatalogError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("AgentSync")
        .build()
        .map_err(|err| CatalogError::Decode(err.to_string()))
}

async fn get(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<Vec<u8>, CatalogError> {
    let mut request = client.get(url);
    for (key, value) in headers {
        request = request.header(key.as_str(), value.as_str());
    }
    let response = request
        .send()
        .await
        .map_err(|err| CatalogError::Decode(err.to_string()))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(CatalogError::Http(status));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|err| CatalogError::Decode(err.to_string()))
}

#[derive(Deserialize)]
struct GptLoadList<T> {
    data: Option<GptLoadItems<T>>,
}

#[derive(Deserialize)]
struct GptLoadItems<T> {
    items: Vec<T>,
}

#[derive(Deserialize)]
struct GptLoadGroup {
    id: Value,
    name: String,
}

#[derive(Deserialize)]
struct GptLoadModel {
    id: Option<String>,
    client_model: Option<String>,
    alias: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiList {
    data: Option<Vec<OpenAiModel>>,
}

#[derive(Deserialize)]
struct OpenAiModel {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct GoogleList {
    #[serde(default)]
    models: Vec<GoogleModel>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct GoogleModel {
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "inputTokenLimit")]
    input_token_limit: Option<i64>,
    #[serde(rename = "outputTokenLimit")]
    output_token_limit: Option<i64>,
    #[serde(rename = "supportedGenerationMethods")]
    supported_generation_methods: Option<Vec<String>>,
    thinking: Option<bool>,
}

fn google_model_id(name: &str) -> String {
    name.strip_prefix("models/").unwrap_or(name).to_string()
}

fn google_entry(item: GoogleModel) -> Option<ModelEntry> {
    if let Some(methods) = &item.supported_generation_methods {
        if !methods.iter().any(|method| method == "generateContent") {
            return None;
        }
    }
    let raw_name = item.name.unwrap_or_default();
    let id = google_model_id(&raw_name);
    if id.is_empty() {
        return None;
    }
    let name = item
        .display_name
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| id.clone());
    let efforts = if item.thinking == Some(false) {
        Some(Vec::new())
    } else {
        None
    };
    Some(ModelEntry {
        id,
        request_id: None,
        name,
        context: item.input_token_limit,
        output: item.output_token_limit,
        input: Vec::new(),
        efforts,
        tool_call: true,
    })
}

async fn fetch_google_models(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<Vec<ModelEntry>, CatalogError> {
    let google_base = endpoint::for_provider(base_url, APIKind::Google);
    if google_base.is_empty() {
        return Err(CatalogError::EmptyUrl);
    }
    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), "AgentSync".to_string());
    if !key.is_empty() {
        headers.insert("x-goog-api-key".to_string(), key.to_string());
    }
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    let mut page_token = String::new();
    loop {
        let mut url = join_url(&google_base, "models?pageSize=1000");
        if !page_token.is_empty() {
            url.push_str("&pageToken=");
            url.push_str(&page_token);
        }
        let data = get(client, &url, &headers).await?;
        let list: GoogleList = serde_json::from_slice(&data)
            .map_err(|err| CatalogError::Decode(err.to_string()))?;
        for item in list.models {
            if let Some(model) = google_entry(item) {
                if seen.insert(model.id.clone()) {
                    models.push(model);
                }
            }
        }
        match list.next_page_token {
            Some(token) if !token.is_empty() && token != page_token => page_token = token,
            _ => break,
        }
    }
    if models.is_empty() {
        Err(CatalogError::NoModels)
    } else {
        Ok(display_name::apply(&models))
    }
}

fn flexible_id(value: &Value) -> String {
    match value {
        Value::Number(number) => {
            if let Some(n) = number.as_i64() {
                n.to_string()
            } else if let Some(n) = number.as_u64() {
                n.to_string()
            } else {
                number.to_string()
            }
        }
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}

async fn fetch_gpt_load(
    client: &reqwest::Client,
    management: &str,
    headers: &HashMap<String, String>,
) -> Result<Option<Vec<ModelEntry>>, CatalogError> {
    let groups_url = join_url(management, "api/groups");
    let data = get(client, &groups_url, headers).await?;
    let items = match serde_json::from_slice::<GptLoadList<GptLoadGroup>>(&data) {
        Ok(list) => list.data.map(|items| items.items).unwrap_or_default(),
        Err(_) => return Ok(None),
    };
    if items.is_empty() {
        return Ok(None);
    }

    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for group in items {
        if group.name.is_empty() {
            continue;
        }
        let models_url = join_url(
            management,
            &format!("api/groups/{}/models", flexible_id(&group.id)),
        );
        let payload = match get(client, &models_url, headers).await {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let entries = serde_json::from_slice::<GptLoadList<GptLoadModel>>(&payload)
            .ok()
            .and_then(|list| list.data)
            .map(|items| items.items)
            .unwrap_or_default();
        for item in entries {
            let id = item.client_model.or(item.id).unwrap_or_default();
            if id.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            let alias = item.alias.unwrap_or_default();
            models.push(ModelEntry::new(
                id.clone(),
                if alias.is_empty() { id } else { alias },
            ));
        }
    }
    Ok(if models.is_empty() { None } else { Some(models) })
}

async fn fetch_openai_models(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<Option<Vec<ModelEntry>>, CatalogError> {
    let data = get(client, url, headers).await?;
    let raw_list = if let Ok(envelope) = serde_json::from_slice::<OpenAiList>(&data) {
        if let Some(list) = envelope.data {
            list
        } else if let Ok(list) = serde_json::from_slice::<Vec<OpenAiModel>>(&data) {
            list
        } else {
            return Ok(None);
        }
    } else if let Ok(list) = serde_json::from_slice::<Vec<OpenAiModel>>(&data) {
        list
    } else {
        return Ok(None);
    };
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for item in raw_list {
        let id = item.id.or(item.name).unwrap_or_default();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        models.push(ModelEntry::new(id.clone(), id));
    }
    Ok(if models.is_empty() { None } else { Some(models) })
}

pub async fn fetch_models(
    base_url: &str,
    auth_key: &str,
    client_key: &str,
    api: APIKind,
) -> Result<Vec<ModelEntry>, CatalogError> {
    let bearer = if auth_key.is_empty() {
        client_key
    } else {
        auth_key
    };
    let client = build_client()?;
    if api == APIKind::Google {
        return fetch_google_models(&client, base_url, bearer).await;
    }
    let ends = CatalogEndpoints::split(base_url)?;
    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), "AgentSync".to_string());
    if !bearer.is_empty() {
        headers.insert("Authorization".to_string(), format!("Bearer {bearer}"));
    }

    let mut last_error = CatalogError::NoModels;
    match fetch_gpt_load(&client, &ends.management, &headers).await {
        Ok(Some(models)) => return Ok(display_name::apply(&models)),
        Ok(None) => {}
        Err(err) => last_error = err,
    }

    let client_headers = if client_key.is_empty() {
        headers.clone()
    } else {
        let mut next = headers.clone();
        next.insert(
            "Authorization".to_string(),
            format!("Bearer {client_key}"),
        );
        next
    };
    let used = if client_key.is_empty() {
        &headers
    } else {
        &client_headers
    };
    for url in [
        join_url(&ends.v1, "models"),
        join_url(&ends.management, "models"),
    ] {
        match fetch_openai_models(&client, &url, used).await {
            Ok(Some(models)) => return Ok(display_name::apply(&models)),
            Ok(None) => {}
            Err(err) => last_error = err,
        }
    }
    Err(last_error)
}

enum ProbeError {
    Auth(u16),
    Other(String),
}

async fn probe_chat(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    body: &Value,
) -> Result<(), ProbeError> {
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|err| ProbeError::Other(err.to_string()))?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(ProbeError::Auth(status));
    }
    if !(200..300).contains(&status) {
        return Err(ProbeError::Other(format!("HTTP {status}")));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| ProbeError::Other(err.to_string()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|err| ProbeError::Other(err.to_string()))?;
    if value.get("choices").and_then(Value::as_array).is_some() {
        Ok(())
    } else {
        Err(ProbeError::Other(format!("HTTP {status}")))
    }
}

pub async fn check_compat(provider: &Provider) -> Result<String, String> {
    if provider.api != APIKind::Google {
        return Err("只有 Google 协议的渠道需要兼容检测".to_string());
    }
    if provider.api_key.trim().is_empty() {
        return Err("先填 API Key".to_string());
    }
    if provider.models.is_empty() {
        return Err("先拉取或添加模型".to_string());
    }
    let candidates = endpoint::google_compat_candidates(&provider.base_url);
    let client = build_client().map_err(|err| err.to_string())?;
    let body = serde_json::json!({
        "model": provider.models[0].wire_id(),
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 1,
        "stream": false,
    });
    let mut last = String::new();
    for candidate in &candidates {
        let url = join_url(candidate, "chat/completions");
        match probe_chat(&client, &url, provider.api_key.trim(), &body).await {
            Ok(()) => return Ok(candidate.clone()),
            Err(ProbeError::Auth(code)) => {
                return Err(format!("兼容检测认证失败（HTTP {code}），检查 API Key"));
            }
            Err(ProbeError::Other(message)) => last = message,
        }
    }
    Err(format!(
        "没有检测到 Chat 兼容接口（已尝试：{}；最后一次：{last}）",
        candidates.join("、")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    struct TestServer {
        base: String,
        addr: std::net::SocketAddr,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect(self.addr);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn start_server<F>(handler: F) -> TestServer
    where
        F: Fn(&str, &HashMap<String, String>, &[u8]) -> (u16, Vec<u8>) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handler = Arc::new(handler);
        let thread = thread::spawn(move || {
            while !stop_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let (path, headers, body) = read_request(&mut stream);
                        let (status, body) = handler(&path, &headers, &body);
                        let reason = match status {
                            200 => "OK",
                            401 => "Unauthorized",
                            403 => "Forbidden",
                            404 => "Not Found",
                            _ => "Error",
                        };
                        let header = format!(
                            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                        let _ = stream.flush();
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        TestServer {
            base: format!("http://{addr}"),
            addr,
            stop,
            thread: Some(thread),
        }
    }

    fn read_request(stream: &mut TcpStream) -> (String, HashMap<String, String>, Vec<u8>) {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 512];
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|window| window == b"\r\n\r\n") || buf.len() > 16_384 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let header_end = buf
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .unwrap_or(buf.len());
        let text = String::from_utf8_lossy(&buf[..header_end.min(buf.len())]);
        let mut lines = text.lines();
        let path = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("")
            .to_string();
        let mut headers = HashMap::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
            }
        }
        let content_length = headers
            .get("content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = buf.get(header_end..).unwrap_or(&[]).to_vec();
        while body.len() < content_length {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&tmp[..n]),
                Err(_) => break,
            }
        }
        body.truncate(content_length);
        (path, headers, body)
    }

    #[test]
    fn split_strips_v1() {
        let ends = CatalogEndpoints::split("https://example.invalid/v1/").unwrap();
        assert_eq!(ends.management, "https://example.invalid");
        assert_eq!(ends.v1, "https://example.invalid/v1");
    }

    #[tokio::test]
    async fn fetch_prefers_gpt_load_groups() {
        let server = start_server(|path, _, _| match path {
            "/api/groups" => (
                200,
                br#"{ "data": { "items": [{ "id": 8, "name": "claude" }] } }"#.to_vec(),
            ),
            "/api/groups/8/models" => (
                200,
                br#"{ "data": { "items": [{ "id": "claude-opus-5-5", "client_model": "claude-opus-5-5", "alias": "Opus 5.5" }] } }"#.to_vec(),
            ),
            _ => (404, b"{}".to_vec()),
        });
        let models = fetch_models(&server.base, "k", "", APIKind::Responses)
            .await
            .unwrap();
        assert_eq!(
            models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
            ["claude-opus-5-5"]
        );
        assert_eq!(models.first().map(|model| model.name.as_str()), Some("Opus 5.5"));
    }

    #[tokio::test]
    async fn fetch_falls_back_to_openai_models() {
        let server = start_server(|path, _, _| match path {
            "/v1/models" => (
                200,
                br#"{ "data": [{ "id": "grok-4.6" }, { "id": "grok-4.6" }] }"#.to_vec(),
            ),
            _ => (404, b"{}".to_vec()),
        });
        let base = format!("{}/v1", server.base);
        let models = fetch_models(&base, "", "", APIKind::Responses)
            .await
            .unwrap();
        assert_eq!(
            models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
            ["grok-4.6"]
        );
        assert_eq!(models.first().map(|model| model.name.as_str()), Some("Grok 4.6"));
    }

    #[tokio::test]
    async fn unauthorized_is_not_reported_as_empty() {
        let server = start_server(|_path, _, _| (401, Vec::new()));
        let err = fetch_models(&server.base, "k", "", APIKind::Responses)
            .await
            .unwrap_err();
        assert_eq!(err, CatalogError::Http(401));
        assert_eq!(err.message(), "目录认证失败（HTTP 401），检查 API Key");
    }

    #[tokio::test]
    async fn fetch_google_models_uses_key_paginates_and_filters() {
        let seen_key = Arc::new(Mutex::new(None::<String>));
        let seen_auth = Arc::new(Mutex::new(None::<String>));
        let key_slot = seen_key.clone();
        let auth_slot = seen_auth.clone();
        let server = start_server(move |path, headers, _| {
            *key_slot.lock().unwrap() = headers.get("x-goog-api-key").cloned();
            *auth_slot.lock().unwrap() = headers.get("authorization").cloned();
            match path {
                "/v1beta/models?pageSize=1000" => (
                    200,
                    br#"{ "models": [
                        { "name": "models/gemini-flash", "displayName": "Gemini Flash", "inputTokenLimit": 1000000, "outputTokenLimit": 8192, "supportedGenerationMethods": ["generateContent"], "thinking": true },
                        { "name": "models/embedding-001", "displayName": "Embedding", "supportedGenerationMethods": ["embedContent"] }
                    ], "nextPageToken": "page2" }"#.to_vec(),
                ),
                "/v1beta/models?pageSize=1000&pageToken=page2" => (
                    200,
                    br#"{ "models": [
                        { "name": "models/gemini-lite", "displayName": "Gemini Lite", "inputTokenLimit": 32000, "outputTokenLimit": 4096, "supportedGenerationMethods": ["generateContent"], "thinking": false }
                    ] }"#.to_vec(),
                ),
                _ => (404, b"{}".to_vec()),
            }
        });
        let models = fetch_models(&server.base, "secret-google", "", APIKind::Google)
            .await
            .unwrap();
        assert_eq!(
            models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
            ["gemini-flash", "gemini-lite"]
        );
        assert_eq!(models[0].name, "Gemini Flash");
        assert_eq!(models[0].context, Some(1_000_000));
        assert_eq!(models[0].output, Some(8192));
        assert_eq!(models[0].efforts, None);
        assert_eq!(models[1].name, "Gemini Lite");
        assert_eq!(models[1].context, Some(32_000));
        assert_eq!(models[1].output, Some(4096));
        assert_eq!(models[1].efforts.as_deref(), Some([].as_slice()));
        assert_eq!(seen_key.lock().unwrap().as_deref(), Some("secret-google"));
        assert!(seen_auth.lock().unwrap().is_none());
    }

    fn google_provider(base: &str, key: &str, model_id: &str) -> Provider {
        Provider {
            id: "g".into(),
            name: "Gemini".into(),
            base_url: base.into(),
            api: APIKind::Google,
            catalog_url: String::new(),
            api_key: key.into(),
            compat_base_url: String::new(),
            models: vec![ModelEntry::new(model_id, "Flash")],
        }
    }

    #[tokio::test]
    async fn check_compat_rejects_non_google_and_missing_fields() {
        let mut provider = google_provider("https://example.invalid", "sk", "flash");
        provider.api = APIKind::Responses;
        assert_eq!(
            check_compat(&provider).await.unwrap_err(),
            "只有 Google 协议的渠道需要兼容检测"
        );
        provider.api = APIKind::Google;
        provider.api_key.clear();
        assert_eq!(check_compat(&provider).await.unwrap_err(), "先填 API Key");
        provider.api_key = "sk".into();
        provider.models.clear();
        assert_eq!(
            check_compat(&provider).await.unwrap_err(),
            "先拉取或添加模型"
        );
    }

    #[tokio::test]
    async fn check_compat_uses_bearer_body_and_falls_back_to_v1() {
        let seen_auth = Arc::new(Mutex::new(None::<String>));
        let seen_body = Arc::new(Mutex::new(None::<Value>));
        let auth_slot = seen_auth.clone();
        let body_slot = seen_body.clone();
        let server = start_server(move |path, headers, body| {
            *auth_slot.lock().unwrap() = headers.get("authorization").cloned();
            *body_slot.lock().unwrap() = serde_json::from_slice(body).ok();
            match path {
                "/v1beta/openai/chat/completions" => (404, b"{}".to_vec()),
                "/v1/chat/completions" => (
                    200,
                    br#"{ "choices": [{ "message": { "content": "ok" } }] }"#.to_vec(),
                ),
                _ => (404, b"{}".to_vec()),
            }
        });
        let mut model = ModelEntry::new("flash", "Flash");
        model.request_id = Some("models/gemini-flash".into());
        let provider = Provider {
            id: "g".into(),
            name: "Gemini".into(),
            base_url: server.base.clone(),
            api: APIKind::Google,
            catalog_url: String::new(),
            api_key: "secret-compat".into(),
            compat_base_url: String::new(),
            models: vec![model],
        };
        let url = check_compat(&provider).await.unwrap();
        assert_eq!(url, format!("{}/v1", server.base));
        assert_eq!(
            seen_auth.lock().unwrap().as_deref(),
            Some("Bearer secret-compat")
        );
        let body = seen_body.lock().unwrap().clone().unwrap();
        assert_eq!(body["model"], "models/gemini-flash");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "ping");
        assert_eq!(body["max_tokens"], 1);
        assert_eq!(body["stream"], false);
    }

    #[tokio::test]
    async fn check_compat_auth_failure_and_all_fail() {
        let server = start_server(|path, _, _| match path {
            "/v1beta/openai/chat/completions" => (401, b"{}".to_vec()),
            _ => (404, b"{}".to_vec()),
        });
        let provider = google_provider(&server.base, "bad-key", "flash");
        assert_eq!(
            check_compat(&provider).await.unwrap_err(),
            "兼容检测认证失败（HTTP 401），检查 API Key"
        );

        let server = start_server(|_, _, _| (404, b"{}".to_vec()));
        let provider = google_provider(&server.base, "sk", "flash");
        let err = check_compat(&provider).await.unwrap_err();
        let openai = format!("{}/v1beta/openai", server.base);
        let v1 = format!("{}/v1", server.base);
        assert!(
            err.contains(&format!("没有检测到 Chat 兼容接口（已尝试：{openai}、{v1}；最后一次：HTTP 404）")),
            "{err}"
        );
    }
}
