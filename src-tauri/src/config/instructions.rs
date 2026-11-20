use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::detector::AgentDetector;
use super::fileio::{expand_user_path, prune_backups, WriteSession};
use super::models::AgentKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstructionPreset {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstructionBinding {
    pub target_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub preset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_synced_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomInstructionTarget {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstructionsState {
    #[serde(default)]
    pub presets: Vec<InstructionPreset>,
    #[serde(default)]
    pub bindings: Vec<InstructionBinding>,
    #[serde(default)]
    pub custom_targets: Vec<CustomInstructionTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstructionSyncStatus {
    Synced,
    Pending,
    Conflict,
    Disabled,
    Uninstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetScanRow {
    pub target_id: String,
    pub name: String,
    pub path: String,
    pub file_name: String,
    pub is_builtin: bool,
    pub kind: String,
    pub installed: bool,
    pub file_exists: bool,
    pub disk_hash: Option<String>,
    pub disk_preview: String,
    pub status: InstructionSyncStatus,
    pub enabled: bool,
    pub preset_id: String,
    pub last_synced_hash: Option<String>,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionBackupEntry {
    pub stamp: String,
    pub backup_path: String,
    pub size_bytes: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstructionSyncOutcome {
    pub written: Vec<String>,
    pub backups: Vec<String>,
    pub updated_bindings: Vec<InstructionBinding>,
}

pub struct BuiltinTargetSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub file_name: &'static str,
    pub rel_path: &'static str,
    pub home_dir: &'static str,
    pub binaries: &'static [&'static str],
    pub env_dir_key: Option<&'static str>,
}

pub const BUILTIN_TARGETS: &[BuiltinTargetSpec] = &[
    BuiltinTargetSpec {
        id: "claude",
        name: "Claude Code",
        file_name: "CLAUDE.md",
        rel_path: ".claude/CLAUDE.md",
        home_dir: ".claude",
        binaries: &["claude"],
        env_dir_key: Some("CLAUDE_CONFIG_DIR"),
    },
    BuiltinTargetSpec {
        id: "openclaude",
        name: "OpenClaude",
        file_name: "CLAUDE.md",
        rel_path: ".openclaude/CLAUDE.md",
        home_dir: ".openclaude",
        binaries: &["openclaude"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "codex",
        name: "Codex",
        file_name: "AGENTS.md",
        rel_path: ".codex/AGENTS.md",
        home_dir: ".codex",
        binaries: &["codex"],
        env_dir_key: Some("CODEX_HOME"),
    },
    BuiltinTargetSpec {
        id: "antigravity",
        name: "Antigravity",
        file_name: "AGENTS.md",
        rel_path: ".gemini/config/AGENTS.md",
        home_dir: ".gemini/antigravity",
        binaries: &["agy"],
        env_dir_key: Some("GEMINI_HOME"),
    },
    BuiltinTargetSpec {
        id: "gemini",
        name: "Gemini CLI",
        file_name: "GEMINI.md",
        rel_path: ".gemini/GEMINI.md",
        home_dir: ".gemini",
        binaries: &["gemini"],
        env_dir_key: Some("GEMINI_HOME"),
    },
    BuiltinTargetSpec {
        id: "opencode",
        name: "OpenCode",
        file_name: "AGENTS.md",
        rel_path: ".config/opencode/AGENTS.md",
        home_dir: ".config/opencode",
        binaries: &["opencode"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "dsh",
        name: "DSH",
        file_name: "AGENTS.md",
        rel_path: ".dsh/AGENTS.md",
        home_dir: ".dsh",
        binaries: &["dsh", "dsh-tui"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "zcode",
        name: "ZCode",
        file_name: "AGENTS.md",
        rel_path: ".zcode/AGENTS.md",
        home_dir: ".zcode",
        binaries: &["zcode"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "grok",
        name: "Grok",
        file_name: "AGENTS.md",
        rel_path: ".grok/AGENTS.md",
        home_dir: ".grok",
        binaries: &["grok"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "pi",
        name: "Pi",
        file_name: "AGENTS.md",
        rel_path: ".pi/AGENTS.md",
        home_dir: ".pi",
        binaries: &["pi"],
        env_dir_key: Some("PI_CODING_AGENT_DIR"),
    },
    BuiltinTargetSpec {
        id: "omp",
        name: "Oh My Pi",
        file_name: "AGENTS.md",
        rel_path: ".omp/AGENTS.md",
        home_dir: ".omp",
        binaries: &["omp"],
        env_dir_key: Some("OMP_AGENT_DIR"),
    },
    BuiltinTargetSpec {
        id: "qwen",
        name: "Qwen",
        file_name: "QWEN.md",
        rel_path: ".qwen/QWEN.md",
        home_dir: ".qwen",
        binaries: &["qwen"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "hermes",
        name: "Hermes",
        file_name: "SOUL.md",
        rel_path: ".hermes/SOUL.md",
        home_dir: ".hermes",
        binaries: &["hermes"],
        env_dir_key: None,
    },
    BuiltinTargetSpec {
        id: "openclaw",
        name: "OpenClaw",
        file_name: "AGENTS.md",
        rel_path: ".openclaw/AGENTS.md",
        home_dir: ".openclaw",
        binaries: &["openclaw"],
        env_dir_key: None,
    },
];

pub fn normalize_instruction_bytes(content: &str) -> Vec<u8> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    let trimmed = content.trim_end_matches(['\n', '\r']);
    format!("{trimmed}\n").into_bytes()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn resolve_builtin_path(
    spec: &BuiltinTargetSpec,
    home: &Path,
    env: &HashMap<String, String>,
    custom_path: Option<&str>,
) -> PathBuf {
    if let Some(cp) = custom_path.map(str::trim).filter(|s| !s.is_empty()) {
        return expand_user_path(cp, home);
    }
    if let Some(env_key) = spec.env_dir_key {
        if let Some(raw) = env.get(env_key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let root = expand_user_path(raw, home);
            if spec.id == "antigravity" {
                return root.join("config").join(spec.file_name);
            }
            return root.join(spec.file_name);
        }
    }
    let primary = home.join(spec.rel_path);
    // For Pi / omp, keep fallback to ~/.pi/AGENTS.md or ~/.omp/AGENTS.md (primary is already that,
    // also support ~/.pi/agent/AGENTS.md if it exists and primary doesn't).
    if spec.id == "pi" && !primary.exists() {
        let alt = home.join(".pi/agent/AGENTS.md");
        if alt.exists() {
            return alt;
        }
    }
    if spec.id == "omp" && !primary.exists() {
        let alt = home.join(".omp/agent/AGENTS.md");
        if alt.exists() {
            return alt;
        }
    }
    primary
}

fn binary_exists(names: &[&str], home: &Path, env: &HashMap<String, String>) -> bool {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".local/bin"),
        home.join("bin"),
    ];
    if let Some(path) = env.get("PATH") {
        dirs.extend(path.split(':').map(PathBuf::from));
    }
    let mut seen = HashSet::new();
    for dir in dirs {
        if !seen.insert(dir.clone()) {
            continue;
        }
        for name in names {
            if dir.join(name).is_file() {
                return true;
            }
        }
    }
    false
}

fn is_builtin_installed(
    spec: &BuiltinTargetSpec,
    home: &Path,
    env: &HashMap<String, String>,
    target_path: &Path,
) -> bool {
    if let Some(kind) = AgentKind::from_raw(spec.id) {
        let detector = AgentDetector::new(env.clone(), home.to_path_buf(), None, None);
        if detector.is_installed(kind) {
            return true;
        }
    }
    if target_path.exists() || home.join(spec.home_dir).is_dir() {
        return true;
    }
    binary_exists(spec.binaries, home, env)
}

pub fn compute_status(
    installed: bool,
    is_builtin: bool,
    enabled: bool,
    preset_content: Option<&str>,
    disk_bytes: Option<&[u8]>,
    last_synced_hash: Option<&str>,
) -> InstructionSyncStatus {
    let file_exists = disk_bytes.is_some();
    if !enabled {
        return if is_builtin && !installed && !file_exists {
            InstructionSyncStatus::Uninstalled
        } else {
            InstructionSyncStatus::Disabled
        };
    }
    let desired = preset_content
        .map(normalize_instruction_bytes)
        .unwrap_or_default();
    if desired.is_empty() {
        return if is_builtin && !installed && !file_exists {
            InstructionSyncStatus::Uninstalled
        } else {
            InstructionSyncStatus::Disabled
        };
    }
    let desired_hash = sha256_hex(&desired);
    match disk_bytes {
        Some(bytes) => {
            let disk_hash = sha256_hex(bytes);
            if disk_hash == desired_hash {
                InstructionSyncStatus::Synced
            } else if last_synced_hash == Some(disk_hash.as_str()) {
                InstructionSyncStatus::Pending
            } else {
                InstructionSyncStatus::Conflict
            }
        }
        None => {
            if is_builtin && !installed {
                InstructionSyncStatus::Uninstalled
            } else {
                InstructionSyncStatus::Pending
            }
        }
    }
}

pub fn scan_targets(
    state: &InstructionsState,
    home: &Path,
    env: &HashMap<String, String>,
) -> Vec<TargetScanRow> {
    let preset_map: HashMap<&str, &InstructionPreset> = state
        .presets
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let binding_map: HashMap<&str, &InstructionBinding> = state
        .bindings
        .iter()
        .map(|b| (b.target_id.as_str(), b))
        .collect();

    let mut rows = Vec::new();
    for spec in BUILTIN_TARGETS {
        let binding = binding_map.get(spec.id).copied();
        let path = resolve_builtin_path(
            spec,
            home,
            env,
            binding.and_then(|b| b.custom_path.as_deref()),
        );
        let installed = is_builtin_installed(spec, home, env, &path);
        let disk_bytes = std::fs::read(&path).ok();
        let disk_hash = disk_bytes.as_deref().map(sha256_hex);
        let disk_preview = disk_bytes
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.chars().take(240).collect::<String>())
            .unwrap_or_default();
        let enabled = binding.map(|b| b.enabled).unwrap_or(false);
        let preset_id = binding.map(|b| b.preset_id.clone()).unwrap_or_default();
        let preset_content = preset_map.get(preset_id.as_str()).map(|p| p.content.as_str());
        let last_synced_hash = binding.and_then(|b| b.last_synced_hash.clone());
        let last_synced_at = binding.and_then(|b| b.last_synced_at.clone());
        let status = compute_status(
            installed,
            true,
            enabled,
            preset_content,
            disk_bytes.as_deref(),
            last_synced_hash.as_deref(),
        );
        rows.push(TargetScanRow {
            target_id: spec.id.to_string(),
            name: spec.name.to_string(),
            path: path.to_string_lossy().into_owned(),
            file_name: spec.file_name.to_string(),
            is_builtin: true,
            kind: "builtin".to_string(),
            installed,
            file_exists: disk_bytes.is_some(),
            disk_hash,
            disk_preview,
            status,
            enabled,
            preset_id,
            last_synced_hash,
            last_synced_at,
        });
    }

    for custom in &state.custom_targets {
        let binding = binding_map.get(custom.id.as_str()).copied();
        let raw_path = binding
            .and_then(|b| b.custom_path.as_deref())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&custom.path);
        let path = expand_user_path(raw_path, home);
        let disk_bytes = std::fs::read(&path).ok();
        let disk_hash = disk_bytes.as_deref().map(sha256_hex);
        let disk_preview = disk_bytes
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(|s| s.chars().take(240).collect::<String>())
            .unwrap_or_default();
        let enabled = binding.map(|b| b.enabled).unwrap_or(false);
        let preset_id = binding.map(|b| b.preset_id.clone()).unwrap_or_default();
        let preset_content = preset_map.get(preset_id.as_str()).map(|p| p.content.as_str());
        let last_synced_hash = binding.and_then(|b| b.last_synced_hash.clone());
        let last_synced_at = binding.and_then(|b| b.last_synced_at.clone());
        let status = compute_status(
            true,
            false,
            enabled,
            preset_content,
            disk_bytes.as_deref(),
            last_synced_hash.as_deref(),
        );
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("AGENTS.md")
            .to_string();
        rows.push(TargetScanRow {
            target_id: custom.id.clone(),
            name: if custom.name.is_empty() {
                custom.id.clone()
            } else {
                custom.name.clone()
            },
            path: path.to_string_lossy().into_owned(),
            file_name,
            is_builtin: false,
            kind: if custom.kind.is_empty() {
                "custom".to_string()
            } else {
                custom.kind.clone()
            },
            installed: true,
            file_exists: disk_bytes.is_some(),
            disk_hash,
            disk_preview,
            status,
            enabled,
            preset_id,
            last_synced_hash,
            last_synced_at,
        });
    }

    rows
}

pub fn resolve_target_path(
    state: &InstructionsState,
    target_id: &str,
    home: &Path,
    env: &HashMap<String, String>,
) -> Option<PathBuf> {
    let binding = state.bindings.iter().find(|b| b.target_id == target_id);
    if let Some(spec) = BUILTIN_TARGETS.iter().find(|s| s.id == target_id) {
        return Some(resolve_builtin_path(
            spec,
            home,
            env,
            binding.and_then(|b| b.custom_path.as_deref()),
        ));
    }
    state
        .custom_targets
        .iter()
        .find(|c| c.id == target_id)
        .map(|custom| {
            let raw = binding
                .and_then(|b| b.custom_path.as_deref())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&custom.path);
            expand_user_path(raw, home)
        })
}

pub fn read_target_file(
    state: &InstructionsState,
    target_id: &str,
    home: &Path,
    env: &HashMap<String, String>,
) -> Result<String, String> {
    let Some(path) = resolve_target_path(state, target_id, home, env) else {
        return Err(format!("Unknown instruction target: {target_id}"));
    };
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))
}

pub fn sync_targets(
    state: &mut InstructionsState,
    target_ids: &[String],
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
) -> Result<InstructionSyncOutcome, String> {
    let mut session = WriteSession::new(backups_root, home);
    let stamp_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut outcome = InstructionSyncOutcome::default();

    let preset_map: HashMap<String, String> = state
        .presets
        .iter()
        .map(|p| (p.id.clone(), p.content.clone()))
        .collect();

    let requested: HashSet<&str> = target_ids.iter().map(String::as_str).collect();
    for binding in &mut state.bindings {
        if !requested.is_empty() && !requested.contains(binding.target_id.as_str()) {
            continue;
        }
        if !binding.enabled || binding.preset_id.is_empty() {
            continue;
        }
        let Some(content) = preset_map.get(&binding.preset_id) else {
            continue;
        };
        let bytes = normalize_instruction_bytes(content);
        if bytes.is_empty() {
            continue;
        }
        let path = if let Some(spec) = BUILTIN_TARGETS.iter().find(|s| s.id == binding.target_id) {
            resolve_builtin_path(spec, home, env, binding.custom_path.as_deref())
        } else if let Some(custom) = state
            .custom_targets
            .iter()
            .find(|c| c.id == binding.target_id)
        {
            let raw = binding
                .custom_path
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&custom.path);
            expand_user_path(raw, home)
        } else {
            continue;
        };

        let hash = sha256_hex(&bytes);
        let changed = session
            .write("instructions", &path, &bytes, 0o644)
            .map_err(|e| e.message())?;
        if changed || binding.last_synced_hash.as_deref() != Some(hash.as_str()) {
            binding.last_synced_hash = Some(hash);
            binding.last_synced_at = Some(stamp_iso.clone());
            outcome.updated_bindings.push(binding.clone());
        }
    }

    if !session.backups.is_empty() {
        prune_backups(backups_root, 20).map_err(|e| e.message())?;
    }

    outcome.written = session
        .written
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    outcome.backups = session
        .backups
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    Ok(outcome)
}

fn backup_relative_path(home: &Path, path: &Path) -> PathBuf {
    if let Ok(rel) = path.strip_prefix(home) {
        if rel.as_os_str().is_empty() {
            return path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("file"));
        }
        return rel.to_path_buf();
    }
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => continue,
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn list_backups(
    state: &InstructionsState,
    target_id: &str,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
) -> Vec<InstructionBackupEntry> {
    let Some(target_path) = resolve_target_path(state, target_id, home, env) else {
        return Vec::new();
    };
    let rel = backup_relative_path(home, &target_path);
    let Ok(stamps) = std::fs::read_dir(backups_root) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for stamp_entry in stamps.flatten() {
        let stamp_path = stamp_entry.path();
        if !stamp_path.is_dir() {
            continue;
        }
        let stamp_name = stamp_entry.file_name().to_string_lossy().into_owned();
        let Ok(agents) = std::fs::read_dir(&stamp_path) else {
            continue;
        };
        for agent_entry in agents.flatten() {
            let candidate = agent_entry.path().join(&rel);
            if candidate.is_file() {
                if let Ok(meta) = std::fs::metadata(&candidate) {
                    let modified_ms = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    entries.push(InstructionBackupEntry {
                        stamp: stamp_name.clone(),
                        backup_path: candidate.to_string_lossy().into_owned(),
                        size_bytes: meta.len(),
                        modified_ms,
                    });
                }
            }
        }
    }
    entries.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    entries
}

pub fn restore_backup(
    state: &InstructionsState,
    target_id: &str,
    backup_path: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
) -> Result<String, String> {
    let canonical_root = std::fs::canonicalize(backups_root)
        .map_err(|_| "Backups directory does not exist".to_string())?;
    let canonical_backup = std::fs::canonicalize(backup_path)
        .map_err(|_| "Backup file does not exist".to_string())?;
    if !canonical_backup.starts_with(&canonical_root) {
        return Err("Backup path is outside backups directory".to_string());
    }
    let Some(target_path) = resolve_target_path(state, target_id, home, env) else {
        return Err(format!("Unknown instruction target: {target_id}"));
    };
    let bytes = std::fs::read(&canonical_backup).map_err(|e| format!("Read backup failed: {e}"))?;
    let mut session = WriteSession::new(backups_root, home);
    session
        .write("instructions-restore", &target_path, &bytes, 0o644)
        .map_err(|e| e.message())?;
    String::from_utf8(bytes).map_err(|_| "Backup file is not valid UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::AppDocument;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_instr_{}_{}",
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

    #[test]
    fn v1_migration_creates_default_preset_and_bindings() {
        let mut doc = AppDocument::default();
        doc.library.instructions = "Always write concise code".into();
        doc.ensure_agent_defaults();
        assert_eq!(doc.instructions.presets.len(), 1);
        assert_eq!(doc.instructions.presets[0].id, "default");
        assert_eq!(doc.instructions.presets[0].content, "Always write concise code");
        assert_eq!(doc.instructions.bindings.len(), AgentKind::PROVIDER_AGENTS.len());
        assert!(doc.instructions.bindings.iter().all(|b| b.enabled && b.preset_id == "default"));
    }

    #[test]
    fn status_detects_pending_synced_and_conflict() {
        let content = "Be careful";
        let normalized = normalize_instruction_bytes(content);
        let h = sha256_hex(&normalized);

        assert_eq!(
            compute_status(true, true, true, Some(content), None, None),
            InstructionSyncStatus::Pending
        );
        assert_eq!(
            compute_status(true, true, true, Some(content), Some(&normalized), Some(&h)),
            InstructionSyncStatus::Synced
        );
        assert_eq!(
            compute_status(true, true, true, Some("Be even more careful"), Some(&normalized), Some(&h)),
            InstructionSyncStatus::Pending
        );
        assert_eq!(
            compute_status(true, true, true, Some(content), Some(b"external edit\n"), Some(&h)),
            InstructionSyncStatus::Conflict
        );
        assert_eq!(
            compute_status(true, true, true, Some(content), Some(b"preexisting\n"), None),
            InstructionSyncStatus::Conflict
        );
    }

    #[test]
    fn sync_creates_backup_and_restore_recovers_previous_content() {
        let dir = TestDir::new("sync_restore");
        let home = dir.path();
        let backups = home.join("backups");
        let claude_dir = home.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");
        std::fs::write(&claude_md, "old claude rules\n").unwrap();

        let mut state = InstructionsState {
            presets: vec![InstructionPreset {
                id: "rust".into(),
                name: "Rust".into(),
                content: "new rust rules".into(),
                ..Default::default()
            }],
            bindings: vec![InstructionBinding {
                target_id: "claude".into(),
                enabled: true,
                preset_id: "rust".into(),
                ..Default::default()
            }],
            custom_targets: Vec::new(),
        };

        let env = HashMap::new();
        let outcome = sync_targets(&mut state, &["claude".into()], home, &env, &backups).unwrap();
        assert_eq!(outcome.written.len(), 1);
        assert_eq!(outcome.backups.len(), 1);
        assert_eq!(std::fs::read_to_string(&claude_md).unwrap(), "new rust rules\n");

        let rows = scan_targets(&state, home, &env);
        let claude_row = rows.iter().find(|r| r.target_id == "claude").unwrap();
        assert_eq!(claude_row.status, InstructionSyncStatus::Synced);

        let list = list_backups(&state, "claude", home, &env, &backups);
        assert_eq!(list.len(), 1);

        let restored = restore_backup(
            &state,
            "claude",
            Path::new(&list[0].backup_path),
            home,
            &env,
            &backups,
        )
        .unwrap();
        assert_eq!(restored, "old claude rules\n");
        assert_eq!(std::fs::read_to_string(&claude_md).unwrap(), "old claude rules\n");
    }
}
