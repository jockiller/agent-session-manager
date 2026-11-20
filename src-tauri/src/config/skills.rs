use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::Local;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::fileio::{atomic_write, expand_user_path, prune_backups};
use super::models::AgentKind;

pub const SOURCE_META_FILE: &str = ".asm-source.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceMeta {
    pub kind: String,
    pub repo: String,
    pub branch: String,
    pub subpath: String,
    pub url: String,
    pub commit: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileInfo {
    pub rel_path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub skill_md_path: String,
    pub content: String,
    pub extra_files: Vec<SkillFileInfo>,
    pub linked_agents: Vec<String>,
    pub updated_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSourceMeta>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDiskSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_path: String,
    pub source_agents: Vec<String>,
    pub in_library: bool,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSyncOutcome {
    pub linked: Vec<String>,
    pub unlinked: Vec<String>,
    pub backups: Vec<String>,
}

fn env_directory(env: &HashMap<String, String>, key: &str, home: &Path, fallback: &str) -> PathBuf {
    if let Some(raw) = env.get(key) {
        if !raw.trim().is_empty() {
            return expand_user_path(raw, home);
        }
    }
    home.join(fallback)
}

pub fn agent_skills_dir(kind: AgentKind, home: &Path, env: &HashMap<String, String>) -> PathBuf {
    match kind {
        AgentKind::Antigravity => env_directory(env, "GEMINI_HOME", home, ".gemini")
            .join("config")
            .join("skills"),
        AgentKind::Claude => env_directory(env, "CLAUDE_CONFIG_DIR", home, ".claude").join("skills"),
        AgentKind::Codex => env_directory(env, "CODEX_HOME", home, ".codex").join("skills"),
        AgentKind::Gemini => env_directory(env, "GEMINI_HOME", home, ".gemini").join("skills"),
        AgentKind::Opencode => home.join(".config/opencode/skills"),
        AgentKind::Cursor => home.join(".cursor/skills"),
        AgentKind::Qwen => env_directory(env, "QWEN_HOME", home, ".qwen").join("skills"),
        AgentKind::Pi => env_directory(env, "PI_CODING_AGENT_DIR", home, ".pi/agent").join("skills"),
        AgentKind::Omp => env_directory(env, "OMP_AGENT_DIR", home, ".omp/agent").join("skills"),
        AgentKind::Dsh => home.join(".dsh/skills"),
        AgentKind::Zcode => home.join(".zcode/skills"),
        AgentKind::Grok => home.join(".grok/skills"),
    }
}

pub fn sanitize_skill_id(raw: &str) -> Result<String, String> {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            slug.push(ch);
            prev_dash = ch == '-';
        } else if (ch.is_whitespace() || ch == '/' || ch == '\\' || ch == '.') && !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let clean = slug.trim_matches('-').to_string();
    if clean.is_empty() || clean == "." || clean == ".." {
        return Err("Skill ID 不能为空且仅支持字母、数字、连字符和下划线".to_string());
    }
    Ok(clean)
}

pub fn parse_frontmatter(content: &str) -> (String, String) {
    let mut lines = content.lines();
    let Some(first) = lines.next() else {
        return (String::new(), String::new());
    };
    if first.trim() != "---" {
        return (String::new(), String::new());
    }
    let mut fm_lines = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        fm_lines.push(line);
    }
    if !closed {
        return (String::new(), String::new());
    }
    let mut name = String::new();
    let mut desc = String::new();
    let mut idx = 0usize;
    while idx < fm_lines.len() {
        let line = fm_lines[idx];
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name:") {
            name = unquote_yaml_scalar(rest);
        } else if let Some(rest) = trimmed.strip_prefix("description:") {
            let val = rest.trim();
            if val == ">" || val == ">-" || val == "|" || val == "|-" {
                let mut parts = Vec::new();
                while idx + 1 < fm_lines.len() {
                    let next = fm_lines[idx + 1];
                    if next.starts_with(' ') || next.starts_with('\t') {
                        parts.push(next.trim().to_string());
                        idx += 1;
                    } else if next.trim().is_empty() {
                        idx += 1;
                    } else {
                        break;
                    }
                }
                desc = parts.join(if val.starts_with('|') { "\n" } else { " " });
            } else {
                desc = unquote_yaml_scalar(val);
            }
        }
        idx += 1;
    }
    (name, desc)
}

fn unquote_yaml_scalar(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

pub(crate) fn canonical_or_normalized(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolved_symlink_target(link: &Path) -> Option<PathBuf> {
    let meta = std::fs::symlink_metadata(link).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let raw = std::fs::read_link(link).ok()?;
    let joined = if raw.is_absolute() {
        raw
    } else {
        link.parent().unwrap_or(Path::new("/")).join(raw)
    };
    Some(joined)
}

fn symlink_points_to(link: &Path, expected: &Path) -> bool {
    let Some(target) = resolved_symlink_target(link) else {
        return false;
    };
    if target == expected {
        return true;
    }
    match (std::fs::canonicalize(&target), std::fs::canonicalize(expected)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn symlink_in_root(link: &Path, root: &Path) -> bool {
    let Some(target) = resolved_symlink_target(link) else {
        return false;
    };
    if target.starts_with(root) {
        return true;
    }
    let canon_root = canonical_or_normalized(root);
    let canon_target = canonical_or_normalized(&target);
    canon_target.starts_with(&canon_root)
}

fn create_dir_symlink(src: &Path, dst: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(src, dst)
    }
}

fn remove_symlink(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path))
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(path)
    }
}

fn should_skip_copy(name: &str) -> bool {
    matches!(name, ".git" | ".DS_Store" | "node_modules" | ".asm-fetched")
}

pub(crate) fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("创建目录 {} 失败：{e}", dst.display()))?;
    let entries = std::fs::read_dir(src)
        .map_err(|e| format!("读取目录 {} 失败：{e}", src.display()))?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if should_skip_copy(&name) {
            continue;
        }
        let child_src = entry.path();
        let child_dst = dst.join(&file_name);
        let meta = std::fs::symlink_metadata(&child_src)
            .map_err(|e| format!("读取 {} 元数据失败：{e}", child_src.display()))?;
        if meta.is_dir() || child_src.is_dir() {
            copy_dir_all(&child_src, &child_dst)?;
        } else {
            std::fs::copy(&child_src, &child_dst)
                .map_err(|e| format!("复制 {} 失败：{e}", child_src.display()))?;
        }
    }
    Ok(())
}

pub(crate) fn backup_existing_entry(src: &Path, backup_dst: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(src)
        .map_err(|e| format!("读取 {} 失败：{e}", src.display()))?;
    if meta.file_type().is_symlink() {
        if src.is_dir() {
            copy_dir_all(src, backup_dst)?;
        }
        remove_symlink(src).map_err(|e| format!("移除软链接 {} 失败：{e}", src.display()))?;
        return Ok(());
    }
    if meta.is_dir() {
        copy_dir_all(src, backup_dst)?;
        std::fs::remove_dir_all(src)
            .map_err(|e| format!("清理原目录 {} 失败：{e}", src.display()))?;
        return Ok(());
    }
    if let Some(parent) = backup_dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建备份目录失败：{e}"))?;
    }
    std::fs::copy(src, backup_dst)
        .map_err(|e| format!("备份文件 {} 失败：{e}", src.display()))?;
    std::fs::remove_file(src)
        .map_err(|e| format!("移除原文件 {} 失败：{e}", src.display()))?;
    Ok(())
}

pub(crate) fn collect_extra_files(skill_dir: &Path) -> Vec<SkillFileInfo> {
    let mut files = Vec::new();
    for entry in WalkDir::new(skill_dir)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !should_skip_copy(&name)
        })
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(skill_dir) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str == "SKILL.md" || rel_str == SOURCE_META_FILE {
            continue;
        }
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        files.push(SkillFileInfo {
            rel_path: rel_str,
            size_bytes,
        });
    }
    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    files
}

pub(crate) fn read_source_meta(skill_dir: &Path) -> Option<SkillSourceMeta> {
    let data = std::fs::read(skill_dir.join(SOURCE_META_FILE)).ok()?;
    serde_json::from_slice(&data).ok()
}

pub(crate) fn write_source_meta(skill_dir: &Path, meta: &SkillSourceMeta) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(meta)
        .map_err(|e| format!("序列化 Skill 来源信息失败：{e}"))?;
    atomic_write(&skill_dir.join(SOURCE_META_FILE), &data, 0o644)
        .map_err(|e| e.message())
}

pub(crate) fn read_skill_entry(
    ssot_root: &Path,
    id: &str,
    home: &Path,
    env: &HashMap<String, String>,
) -> Option<SkillEntry> {
    let dir = ssot_root.join(id);
    let md_path = dir.join("SKILL.md");
    if !md_path.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(&md_path).ok()?;
    let (fm_name, description) = parse_frontmatter(&content);
    let name = if fm_name.is_empty() {
        id.to_string()
    } else {
        fm_name
    };
    let updated_ms = std::fs::metadata(&md_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let extra_files = collect_extra_files(&dir);
    let source = read_source_meta(&dir);
    let mut linked_agents = Vec::new();
    for kind in AgentKind::ALL {
        let link = agent_skills_dir(kind, home, env).join(id);
        if symlink_points_to(&link, &dir) {
            linked_agents.push(kind.raw().to_string());
        }
    }
    Some(SkillEntry {
        id: id.to_string(),
        name,
        description,
        path: dir.to_string_lossy().into_owned(),
        skill_md_path: md_path.to_string_lossy().into_owned(),
        content,
        extra_files,
        linked_agents,
        updated_ms,
        source,
    })
}

pub fn list_skills(ssot_root: &Path, home: &Path, env: &HashMap<String, String>) -> Vec<SkillEntry> {
    let Ok(entries) = std::fs::read_dir(ssot_root) else {
        return Vec::new();
    };
    let mut list = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if let Some(item) = read_skill_entry(ssot_root, &id, home, env) {
            list.push(item);
        }
    }
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

pub fn save_skill(
    ssot_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    raw_id: &str,
    content: &str,
) -> Result<SkillEntry, String> {
    let id = sanitize_skill_id(raw_id)?;
    let md_path = ssot_root.join(&id).join("SKILL.md");
    let trimmed = content.trim_end_matches(['\n', '\r']);
    let body = if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    };
    atomic_write(&md_path, body.as_bytes(), 0o644).map_err(|e| e.message())?;
    read_skill_entry(ssot_root, &id, home, env)
        .ok_or_else(|| format!("读取保存后的 Skill 失败：{}", md_path.display()))
}

pub fn sync_skill_links(
    ssot_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
    raw_id: &str,
    target_agents: &[String],
) -> Result<(SkillEntry, SkillSyncOutcome), String> {
    let id = sanitize_skill_id(raw_id)?;
    let src = ssot_root.join(&id);
    if !src.join("SKILL.md").is_file() {
        return Err(format!("Skill「{id}」不存在或缺少 SKILL.md"));
    }

    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut outcome = SkillSyncOutcome::default();

    for kind in AgentKind::ALL {
        let dst = agent_skills_dir(kind, home, env).join(&id);
        let want_link = target_agents.iter().any(|t| t == kind.raw());
        let exists = std::fs::symlink_metadata(&dst).is_ok();

        if want_link {
            if exists {
                if symlink_points_to(&dst, &src) {
                    continue;
                }
                if symlink_in_root(&dst, ssot_root) {
                    remove_symlink(&dst)
                        .map_err(|e| format!("清理旧软链接 {} 失败：{e}", dst.display()))?;
                } else {
                    let backup_dst = backups_root
                        .join(&stamp)
                        .join("skills")
                        .join(kind.raw())
                        .join(&id);
                    backup_existing_entry(&dst, &backup_dst)?;
                    outcome.backups.push(backup_dst.to_string_lossy().into_owned());
                }
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录 {} 失败：{e}", parent.display()))?;
            }
            create_dir_symlink(&src, &dst).map_err(|e| {
                format!(
                    "创建软链接 {} -> {} 失败：{e}",
                    dst.display(),
                    src.display()
                )
            })?;
            outcome.linked.push(dst.to_string_lossy().into_owned());
        } else if exists && (symlink_points_to(&dst, &src) || symlink_in_root(&dst, ssot_root)) {
            remove_symlink(&dst).map_err(|e| format!("移除软链接 {} 失败：{e}", dst.display()))?;
            outcome.unlinked.push(dst.to_string_lossy().into_owned());
        }
    }

    if !outcome.backups.is_empty() {
        prune_backups(backups_root, 20).map_err(|e| e.message())?;
    }

    let entry = read_skill_entry(ssot_root, &id, home, env)
        .ok_or_else(|| format!("读取 Skill「{id}」失败"))?;
    Ok((entry, outcome))
}

pub fn delete_skill(
    ssot_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    raw_id: &str,
    use_trash: bool,
) -> Result<Vec<String>, String> {
    let id = sanitize_skill_id(raw_id)?;
    let src = ssot_root.join(&id);
    let mut unlinked = Vec::new();

    for kind in AgentKind::ALL {
        let dst = agent_skills_dir(kind, home, env).join(&id);
        if std::fs::symlink_metadata(&dst).is_ok()
            && (symlink_points_to(&dst, &src) || symlink_in_root(&dst, ssot_root))
        {
            remove_symlink(&dst).map_err(|e| format!("移除软链接 {} 失败：{e}", dst.display()))?;
            unlinked.push(dst.to_string_lossy().into_owned());
        }
    }

    if src.exists() {
        if use_trash {
            trash::delete(&src).map_err(|e| format!("移入废纸篓失败：{e}"))?;
        } else {
            std::fs::remove_dir_all(&src)
                .map_err(|e| format!("删除目录 {} 失败：{e}", src.display()))?;
        }
    }

    Ok(unlinked)
}

pub fn scan_disk_skills(
    ssot_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
) -> Vec<DiscoveredDiskSkill> {
    let mut scan_roots: Vec<(String, PathBuf)> = AgentKind::ALL
        .iter()
        .map(|kind| (kind.raw().to_string(), agent_skills_dir(*kind, home, env)))
        .collect();
    scan_roots.push(("shared_agents".to_string(), home.join(".agents/skills")));

    let mut by_id: BTreeMap<String, DiscoveredDiskSkill> = BTreeMap::new();
    for (tag, dir) in scan_roots {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            if symlink_in_root(&child, ssot_root) {
                continue;
            }
            let md_path = child.join("SKILL.md");
            if !md_path.is_file() {
                continue;
            }
            let Some(raw_name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let Ok(id) = sanitize_skill_id(&raw_name) else {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(&md_path) else {
                continue;
            };
            let (fm_name, description) = parse_frontmatter(&content);
            let name = if fm_name.is_empty() { id.clone() } else { fm_name };
            let in_library = ssot_root.join(&id).join("SKILL.md").is_file();
            let file_count = collect_extra_files(&child).len() + 1;

            let item = by_id.entry(id.clone()).or_insert_with(|| DiscoveredDiskSkill {
                id,
                name,
                description,
                source_path: child.to_string_lossy().into_owned(),
                source_agents: Vec::new(),
                in_library,
                file_count,
            });
            if !item.source_agents.contains(&tag) {
                item.source_agents.push(tag.clone());
            }
        }
    }
    by_id.into_values().collect()
}

pub fn import_skill_dir(
    ssot_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
    source_path: &str,
    preferred_id: Option<&str>,
    link_agents: &[String],
) -> Result<(SkillEntry, SkillSyncOutcome), String> {
    let src = expand_user_path(source_path, home);
    if !src.is_dir() || !src.join("SKILL.md").is_file() {
        return Err(format!("所选目录缺少 SKILL.md：{}", src.display()));
    }
    if canonical_or_normalized(&src).starts_with(canonical_or_normalized(ssot_root)) {
        return Err("该目录已在 ASM Skill 库中".to_string());
    }

    let raw_id = preferred_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| src.file_name().and_then(|n| n.to_str()))
        .ok_or_else(|| "无法推导 Skill ID".to_string())?;
    let id = sanitize_skill_id(raw_id)?;
    let dst = ssot_root.join(&id);
    let mut existing_links: Vec<String> = if dst.exists() {
        read_skill_entry(ssot_root, &id, home, env)
            .map(|e| e.linked_agents)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut extra_backups = Vec::new();
    if dst.exists() {
        let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let lib_backup = backups_root.join(&stamp).join("skills").join("library").join(&id);
        backup_existing_entry(&dst, &lib_backup)?;
        extra_backups.push(lib_backup.to_string_lossy().into_owned());
    }

    copy_dir_all(&src, &dst)?;
    for raw in link_agents {
        if AgentKind::from_raw(raw).is_some() && !existing_links.contains(raw) {
            existing_links.push(raw.clone());
        }
    }
    let (entry, mut outcome) =
        sync_skill_links(ssot_root, home, env, backups_root, &id, &existing_links)?;
    outcome.backups.extend(extra_backups);
    Ok((entry, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_skills_{}_{}",
                name,
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
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
    fn frontmatter_parses_single_and_multiline_yaml() {
        let md = "---\nname: code-review\ndescription: >-\n  Multi line\n  description text.\n---\n\n# Body\n";
        let (name, desc) = parse_frontmatter(md);
        assert_eq!(name, "code-review");
        assert_eq!(desc, "Multi line description text.");

        let md2 = "---\nname: 'simple'\ndescription: \"One line\"\n---\n";
        let (n2, d2) = parse_frontmatter(md2);
        assert_eq!(n2, "simple");
        assert_eq!(d2, "One line");
    }

    #[test]
    fn save_sync_unlink_and_delete_skill_symlinks_safely() {
        let dir = TestDir::new("sync_unlink");
        let root = dir.path();
        let ssot = root.join("app_data/skills");
        let home = root.join("home");
        let backups = root.join("app_data/backups");
        let env = HashMap::new();
        std::fs::create_dir_all(&home).unwrap();

        // Pre-populate conflicting real dir in Antigravity & foreign dir in Codex
        let agy_conflict = agent_skills_dir(AgentKind::Antigravity, &home, &env).join("demo");
        std::fs::create_dir_all(&agy_conflict).unwrap();
        std::fs::write(agy_conflict.join("SKILL.md"), "# Old Conflict\n").unwrap();

        let codex_foreign = agent_skills_dir(AgentKind::Codex, &home, &env).join("demo");
        std::fs::create_dir_all(&codex_foreign).unwrap();
        std::fs::write(codex_foreign.join("SKILL.md"), "# Foreign Codex\n").unwrap();

        let saved = save_skill(
            &ssot,
            &home,
            &env,
            "demo",
            "---\nname: Demo\ndescription: Test skill\n---\n\n# Demo\n",
        )
        .unwrap();
        assert_eq!(saved.id, "demo");

        let (synced, out1) = sync_skill_links(
            &ssot,
            &home,
            &env,
            &backups,
            "demo",
            &["antigravity".into(), "claude".into()],
        )
        .unwrap();
        assert_eq!(synced.linked_agents, vec!["antigravity".to_string(), "claude".to_string()]);
        assert_eq!(out1.backups.len(), 1);
        assert!(Path::new(&out1.backups[0]).join("SKILL.md").is_file());

        let claude_link = agent_skills_dir(AgentKind::Claude, &home, &env).join("demo");
        assert!(std::fs::symlink_metadata(&agy_conflict).unwrap().file_type().is_symlink());
        assert!(std::fs::symlink_metadata(&claude_link).unwrap().file_type().is_symlink());

        // Edits in SSOT reflect through symlinks immediately
        save_skill(&ssot, &home, &env, "demo", "---\nname: Demo\ndescription: Updated\n---\n\n# V2\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(claude_link.join("SKILL.md")).unwrap(),
            "---\nname: Demo\ndescription: Updated\n---\n\n# V2\n"
        );

        // Foreign Codex real dir stays untouched; unlinking claude removes only symlink
        let (after_unlink, out2) = sync_skill_links(
            &ssot,
            &home,
            &env,
            &backups,
            "demo",
            &["antigravity".into()],
        )
        .unwrap();
        assert_eq!(after_unlink.linked_agents, vec!["antigravity".to_string()]);
        assert_eq!(out2.unlinked.len(), 1);
        assert!(std::fs::symlink_metadata(&claude_link).is_err());
        assert!(codex_foreign.join("SKILL.md").is_file());

        let deleted_links = delete_skill(&ssot, &home, &env, "demo", false).unwrap();
        assert_eq!(deleted_links.len(), 1);
        assert!(std::fs::symlink_metadata(&agy_conflict).is_err());
        assert!(!ssot.join("demo").exists());
    }

    #[test]
    fn scan_and_import_disk_skill_copies_auxiliary_files_and_symlinks_back() {
        let dir = TestDir::new("scan_import");
        let root = dir.path();
        let ssot = root.join("app_data/skills");
        let home = root.join("home");
        let backups = root.join("app_data/backups");
        let env = HashMap::new();

        let disk_skill = agent_skills_dir(AgentKind::Claude, &home, &env).join("deploy-helper");
        std::fs::create_dir_all(disk_skill.join("scripts")).unwrap();
        std::fs::write(
            disk_skill.join("SKILL.md"),
            "---\nname: Deploy Helper\ndescription: Ship builds\n---\n\nRun script.\n",
        )
        .unwrap();
        std::fs::write(disk_skill.join("scripts/run.sh"), "#!/bin/sh\necho hi\n").unwrap();

        let found = scan_disk_skills(&ssot, &home, &env);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "deploy-helper");
        assert_eq!(found[0].source_agents, vec!["claude".to_string()]);
        assert_eq!(found[0].file_count, 2);
        assert!(!found[0].in_library);

        let (imported, outcome) = import_skill_dir(
            &ssot,
            &home,
            &env,
            &backups,
            &found[0].source_path,
            Some(&found[0].id),
            &found[0].source_agents,
        )
        .unwrap();
        assert_eq!(imported.name, "Deploy Helper");
        assert_eq!(imported.extra_files.len(), 1);
        assert_eq!(imported.extra_files[0].rel_path, "scripts/run.sh");
        assert_eq!(imported.linked_agents, vec!["claude".to_string()]);
        assert_eq!(outcome.backups.len(), 1);
        assert!(std::fs::symlink_metadata(&disk_skill).unwrap().file_type().is_symlink());
        assert!(ssot.join("deploy-helper/scripts/run.sh").is_file());
    }
}
