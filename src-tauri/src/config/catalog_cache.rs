use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::fileio;
use super::models::ModelEntry;
use super::profile::{self, CatalogHit};

const TTL: Duration = Duration::from_secs(86_400);
const MODELS_DEV_URL: &str = "https://models.dev/api.json";

struct Inner {
    index: HashMap<String, CatalogHit>,
    loaded_at: Option<SystemTime>,
}

pub struct ModelCatalog {
    inner: Mutex<Inner>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                index: HashMap::new(),
                loaded_at: None,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn snapshot(&self) -> HashMap<String, CatalogHit> {
        self.lock().index.clone()
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<ModelEntry> {
        let inner = self.lock();
        profile::search(query, &inner.index, limit)
    }

    pub async fn refresh_if_needed(&self, cache_dir: &Path) {
        let now = SystemTime::now();
        {
            let inner = self.lock();
            if let Some(loaded_at) = inner.loaded_at {
                if within_ttl(loaded_at, now) && !inner.index.is_empty() {
                    return;
                }
            }
        }

        let cache = cache_dir.join("models-dev.json");
        if let Ok(data) = std::fs::read(&cache) {
            if let Ok(modified) = std::fs::metadata(&cache).and_then(|meta| meta.modified()) {
                if within_ttl(modified, now) {
                    if let Ok(value) = serde_json::from_slice::<Value>(&data) {
                        if value.is_object() {
                            let index = profile::index(&value);
                            let nonempty = !index.is_empty();
                            {
                                let mut inner = self.lock();
                                inner.index = index;
                                inner.loaded_at = Some(modified);
                            }
                            if nonempty {
                                return;
                            }
                        }
                    }
                }
            }
        }

        let _ = self.fetch_and_store(cache_dir).await;
    }

    pub async fn force_refresh(&self, cache_dir: &Path) -> Result<usize, String> {
        self.fetch_and_store(cache_dir).await
    }

    async fn fetch_and_store(&self, cache_dir: &Path) -> Result<usize, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("AgentSync")
            .build()
            .map_err(|err| format!("创建 HTTP 客户端失败：{err}"))?;
        let response = client
            .get(MODELS_DEV_URL)
            .send()
            .await
            .map_err(|err| format!("请求 models.dev 失败：{err}"))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("请求 models.dev 失败（HTTP {status}）"));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|err| format!("读取 models.dev 响应失败：{err}"))?;
        self.apply_bytes(cache_dir, &bytes)
    }

    fn apply_bytes(&self, cache_dir: &Path, bytes: &[u8]) -> Result<usize, String> {
        let value = serde_json::from_slice::<Value>(bytes)
            .map_err(|err| format!("解析 models.dev 数据失败：{err}"))?;
        if !value.is_object() {
            return Err("models.dev 返回格式异常".to_string());
        }
        let index = profile::index(&value);
        if index.is_empty() {
            return Err("models.dev 返回了空目录".to_string());
        }
        let count = index.len();
        {
            let mut inner = self.lock();
            inner.index = index;
            inner.loaded_at = Some(SystemTime::now());
        }
        let cache = cache_dir.join("models-dev.json");
        let _ = fileio::atomic_write(&cache, bytes, 0o644);
        Ok(count)
    }
}

pub fn global() -> &'static ModelCatalog {
    static GLOBAL: OnceLock<ModelCatalog> = OnceLock::new();
    GLOBAL.get_or_init(ModelCatalog::new)
}

fn within_ttl(then: SystemTime, now: SystemTime) -> bool {
    match now.duration_since(then) {
        Ok(elapsed) => elapsed < TTL,
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_test_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::create_dir_all(&path);
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn uses_fresh_file_cache() {
        let dir = TestDir::new("catalog_cache");
        let json = serde_json::json!({
            "anthropic": {
                "models": {
                    "claude-opus-5-5": {
                        "limit": { "context": 200000, "output": 8192 },
                        "modalities": { "input": ["text", "image", "pdf"] }
                    }
                }
            }
        });
        std::fs::write(
            dir.path().join("models-dev.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        let catalog = ModelCatalog::new();
        catalog.refresh_if_needed(dir.path()).await;
        let hit = catalog
            .snapshot()
            .get("claude-opus-5-5")
            .cloned()
            .expect("indexed");
        assert_eq!(hit.context, Some(200000));
        assert_eq!(hit.output, Some(8192));
    }

    #[test]
    fn apply_bytes_updates_index_and_overwrites_cache() {
        let dir = TestDir::new("catalog_apply");
        let old = serde_json::json!({
            "anthropic": {
                "models": {
                    "claude-old": { "limit": { "context": 100000 } }
                }
            }
        });
        let fresh = serde_json::json!({
            "anthropic": {
                "models": {
                    "claude-new": { "limit": { "context": 250000, "output": 16384 } }
                }
            }
        });
        let catalog = ModelCatalog::new();
        let old_count = catalog
            .apply_bytes(dir.path(), &serde_json::to_vec(&old).unwrap())
            .unwrap();
        assert_eq!(old_count, 1);

        let new_count = catalog
            .apply_bytes(dir.path(), &serde_json::to_vec(&fresh).unwrap())
            .unwrap();
        assert_eq!(new_count, 1);

        let snap = catalog.snapshot();
        assert!(snap.get("claude-old").is_none());
        let hit = snap.get("claude-new").expect("updated model");
        assert_eq!(hit.context, Some(250000));
        assert_eq!(hit.output, Some(16384));

        let cached = std::fs::read(dir.path().join("models-dev.json")).unwrap();
        let parsed: Value = serde_json::from_slice(&cached).unwrap();
        assert!(parsed["anthropic"]["models"]["claude-new"].is_object());
    }
}
