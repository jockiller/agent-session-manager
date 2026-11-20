use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use chrono::{Local, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::fileio::{atomic_write, prune_backups};
use super::skills::{
    self, SkillEntry, SkillSourceMeta, SkillSyncOutcome, backup_existing_entry,
    canonical_or_normalized, collect_extra_files, copy_dir_all, parse_frontmatter,
    read_skill_entry, read_source_meta, sanitize_skill_id, write_source_meta,
};

const REPOS_FILE: &str = "skill-repos.json";
const FETCH_STAMP_FILE: &str = ".asm-fetched";
const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);
const MAX_DIRECT_URL_BYTES: usize = 1024 * 1024;
const MAX_PREVIEW_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRepo {
    pub id: String,
    pub label: String,
    pub url: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSkillCandidate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub repo_id: String,
    pub repo_label: String,
    pub repo_url: String,
    pub branch: String,
    pub subpath: String,
    pub commit: String,
    pub source_kind: String,
    pub cache_dir: String,
    pub skill_md_preview: String,
    pub extra_files: Vec<String>,
    pub has_scripts: bool,
    pub in_library: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateOutcome {
    pub entry: SkillEntry,
    pub updated: bool,
    pub previous_commit: String,
    pub current_commit: String,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedRemoteInput {
    DirectMarkdownUrl {
        url: String,
        suggested_id: Option<String>,
    },
    GitRepo {
        repo_id: String,
        clone_url: String,
        branch: String,
        subpath: String,
        skill_filter: Option<String>,
    },
}

pub fn default_repos() -> Vec<SkillRepo> {
    vec![
        SkillRepo {
            id: "anthropics/skills".into(),
            label: "Anthropic Official".into(),
            url: "https://github.com/anthropics/skills.git".into(),
            branch: String::new(),
            enabled: true,
            builtin: true,
        },
        SkillRepo {
            id: "vercel-labs/agent-skills".into(),
            label: "Vercel Agent Skills".into(),
            url: "https://github.com/vercel-labs/agent-skills.git".into(),
            branch: String::new(),
            enabled: true,
            builtin: true,
        },
        SkillRepo {
            id: "openai/skills".into(),
            label: "OpenAI Skills".into(),
            url: "https://github.com/openai/skills.git".into(),
            branch: String::new(),
            enabled: true,
            builtin: true,
        },
    ]
}

pub fn load_repos(app_data: &Path) -> Vec<SkillRepo> {
    let path = app_data.join(REPOS_FILE);
    let Ok(data) = std::fs::read(&path) else {
        return default_repos();
    };
    match serde_json::from_slice::<Vec<SkillRepo>>(&data) {
        Ok(list) if !list.is_empty() => normalize_repos(list),
        _ => default_repos(),
    }
}

pub fn save_repos(app_data: &Path, repos: Vec<SkillRepo>) -> Result<Vec<SkillRepo>, String> {
    let normalized = normalize_repos(repos);
    let data = serde_json::to_vec_pretty(&normalized)
        .map_err(|e| format!("序列化仓库列表失败：{e}"))?;
    atomic_write(&app_data.join(REPOS_FILE), &data, 0o644).map_err(|e| e.message())?;
    Ok(normalized)
}

fn normalize_repos(repos: Vec<SkillRepo>) -> Vec<SkillRepo> {
    let builtin_ids: HashSet<String> = default_repos().into_iter().map(|r| r.id).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for mut repo in repos {
        let raw_url = if repo.url.trim().is_empty() {
            repo.id.trim().to_string()
        } else {
            repo.url.trim().to_string()
        };
        if let Ok(ParsedRemoteInput::GitRepo { repo_id, clone_url, branch, .. }) =
            parse_remote_input(&raw_url)
        {
            if repo.id.trim().is_empty() {
                repo.id = repo_id;
            }
            repo.url = clone_url;
            if repo.branch.trim().is_empty() && !branch.is_empty() {
                repo.branch = branch;
            }
        }
        repo.id = repo.id.trim().to_string();
        if repo.id.is_empty() || repo.url.is_empty() || !seen.insert(repo.id.clone()) {
            continue;
        }
        if repo.label.trim().is_empty() {
            repo.label = repo.id.clone();
        }
        repo.builtin = builtin_ids.contains(&repo.id);
        out.push(repo);
    }
    out
}

fn strip_cli_prefix(raw: &str) -> (String, Option<String>) {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens.is_empty() {
        return (String::new(), None);
    }
    let mut i = 0usize;
    if matches!(tokens[0], "npx" | "pnpx" | "bunx" | "yarn") {
        i += 1;
        while i < tokens.len() && tokens[i].starts_with('-') {
            i += 1;
        }
    } else if tokens.len() >= 2 && matches!((tokens[0], tokens[1]), ("npm", "exec") | ("pnpm", "dlx")) {
        i = 2;
        while i < tokens.len() && tokens[i].starts_with('-') {
            i += 1;
        }
    }
    if i < tokens.len() && tokens[i] == "skills" {
        i += 1;
        if i < tokens.len() && tokens[i] == "add" {
            i += 1;
        }
    } else if i > 0 {
        i = 0;
    }

    let mut target = String::new();
    let mut skill_filter: Option<String> = None;
    let mut idx = i;
    while idx < tokens.len() {
        let tok = tokens[idx].trim_matches(|c| c == '\'' || c == '"');
        if tok == "--skill" || tok == "-s" {
            if let Some(next) = tokens.get(idx + 1) {
                skill_filter = Some(next.trim_matches(|c| c == '\'' || c == '"').to_string());
                idx += 2;
                continue;
            }
        } else if let Some(val) = tok.strip_prefix("--skill=") {
            skill_filter = Some(val.trim_matches(|c| c == '\'' || c == '"').to_string());
        } else if tok == "--agent" || tok == "-a" {
            idx += 2;
            continue;
        } else if !tok.starts_with('-') && target.is_empty() {
            target = tok.to_string();
        } else if tok.starts_with('-') && target.is_empty() && i == 0 {
            return (tok.to_string(), None);
        }
        idx += 1;
    }
    if target.is_empty() {
        (raw.trim().trim_matches(|c| c == '\'' || c == '"').to_string(), skill_filter)
    } else {
        (target, skill_filter)
    }
}

fn suggested_id_from_url(url: &str) -> Option<String> {
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let segs: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
    let last = *segs.last()?;
    if last.eq_ignore_ascii_case("SKILL.md") {
        segs.get(segs.len().checked_sub(2)?).and_then(|s| sanitize_skill_id(s).ok())
    } else {
        sanitize_skill_id(last.strip_suffix(".md").unwrap_or(last)).ok()
    }
}

pub fn parse_remote_input(raw: &str) -> Result<ParsedRemoteInput, String> {
    let (token, skill_filter) = strip_cli_prefix(raw);
    if token.is_empty() {
        return Err("请输入 GitHub 仓库、技能链接或 SKILL.md URL".to_string());
    }
    if token.starts_with('-') {
        return Err("非法输入：不允许命令行参数前缀".to_string());
    }
    let lower = token.to_ascii_lowercase();
    if lower.starts_with("file://") || lower.starts_with("javascript:") || lower.starts_with("data:") {
        return Err("出于安全限制，不支持该 URL 协议".to_string());
    }

    if let Some(rest) = token
        .strip_prefix("https://github.com/")
        .or_else(|| token.strip_prefix("http://github.com/"))
    {
        let clean = rest.split(['?', '#']).next().unwrap_or(rest).trim_end_matches('/');
        let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            let owner = parts[0];
            let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
            let clone_url = format!("https://github.com/{owner}/{repo}.git");
            let repo_id = format!("{owner}/{repo}");
            if parts.len() >= 4 && (parts[2] == "tree" || parts[2] == "blob") {
                let branch = parts[3].to_string();
                let mut sub_parts: Vec<&str> = parts[4..].to_vec();
                if parts[2] == "blob" {
                    if let Some(last) = sub_parts.last().copied() {
                        if last.eq_ignore_ascii_case("SKILL.md") {
                            sub_parts.pop();
                        } else if last.to_ascii_lowercase().ends_with(".md") {
                            let file_path = parts[4..].join("/");
                            let raw_url = format!(
                                "https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{file_path}"
                            );
                            return Ok(ParsedRemoteInput::DirectMarkdownUrl {
                                suggested_id: suggested_id_from_url(&raw_url),
                                url: raw_url,
                            });
                        }
                    }
                }
                return Ok(ParsedRemoteInput::GitRepo {
                    repo_id,
                    clone_url,
                    branch,
                    subpath: sub_parts.join("/"),
                    skill_filter,
                });
            }
            return Ok(ParsedRemoteInput::GitRepo {
                repo_id,
                clone_url,
                branch: String::new(),
                subpath: String::new(),
                skill_filter,
            });
        }
    }

    if lower.starts_with("http://") || lower.starts_with("https://") {
        let path_part = lower.split(['?', '#']).next().unwrap_or(&lower);
        let is_raw = lower.contains("raw.githubusercontent.com/")
            || lower.contains("gist.githubusercontent.com/");
        if path_part.ends_with(".md") || (is_raw && !path_part.ends_with(".git")) {
            return Ok(ParsedRemoteInput::DirectMarkdownUrl {
                suggested_id: suggested_id_from_url(&token),
                url: token,
            });
        }
        let repo_id = token
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .split('/')
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");
        return Ok(ParsedRemoteInput::GitRepo {
            repo_id: if repo_id.is_empty() { token.clone() } else { repo_id },
            clone_url: token,
            branch: String::new(),
            subpath: String::new(),
            skill_filter,
        });
    }

    if token.starts_with("git@") {
        let repo_id = token
            .split(':')
            .nth(1)
            .unwrap_or(&token)
            .trim_end_matches(".git")
            .to_string();
        return Ok(ParsedRemoteInput::GitRepo {
            repo_id,
            clone_url: token,
            branch: String::new(),
            subpath: String::new(),
            skill_filter,
        });
    }

    // Shorthand: owner/repo[@branch][/subpath]
    let segs: Vec<&str> = token.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() >= 2 && !segs[0].contains(':') {
        let owner = segs[0];
        let (repo_raw, branch) = match segs[1].split_once('@') {
            Some((r, b)) => (r, b.to_string()),
            None => (segs[1], String::new()),
        };
        let repo = repo_raw.strip_suffix(".git").unwrap_or(repo_raw);
        if !owner.is_empty() && !repo.is_empty() {
            let repo_id = format!("{owner}/{repo}");
            let clone_url = format!("https://github.com/{repo_id}.git");
            let subpath = segs[2..].join("/");
            return Ok(ParsedRemoteInput::GitRepo {
                repo_id,
                clone_url,
                branch,
                subpath,
                skill_filter,
            });
        }
    }

    Err(format!("无法识别的 Skill 地址：{token}"))
}

fn cache_key(url: &str, branch: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.update(b"::");
    hasher.update(branch.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

fn repo_cache_dir(cache_root: &Path, clone_url: &str, branch: &str) -> PathBuf {
    cache_root
        .join("skill-repos")
        .join(cache_key(clone_url, branch))
}

fn is_cache_fresh(dir: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(dir.join(FETCH_STAMP_FILE)) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < CACHE_TTL)
        .unwrap_or(false)
}

fn run_git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .map_err(|e| format!("无法执行 git：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git 执行失败：退出码 {:?}", output.status.code())
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_head_info(dir: &Path) -> (String, String) {
    let dir_str = dir.to_string_lossy();
    let commit = run_git(&["-C", &dir_str, "rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let branch =
        run_git(&["-C", &dir_str, "rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    (commit, branch)
}

pub(crate) fn git_sync_repo(
    clone_url: &str,
    branch: &str,
    cache_dir: &Path,
    force_refresh: bool,
) -> Result<(String, String), String> {
    if cache_dir.join(".git").is_dir() {
        if !force_refresh && is_cache_fresh(cache_dir) {
            return Ok(git_head_info(cache_dir));
        }
        let dir_str = cache_dir.to_string_lossy();
        let fetched = if branch.trim().is_empty() {
            run_git(&["-C", &dir_str, "fetch", "--depth", "1", "origin"])
        } else {
            run_git(&["-C", &dir_str, "fetch", "--depth", "1", "origin", branch.trim()])
        };
        if fetched.is_ok() && run_git(&["-C", &dir_str, "reset", "--hard", "FETCH_HEAD"]).is_ok() {
            let _ = std::fs::write(cache_dir.join(FETCH_STAMP_FILE), b"ok");
            return Ok(git_head_info(cache_dir));
        }
        let _ = std::fs::remove_dir_all(cache_dir);
    }

    if let Some(parent) = cache_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建缓存目录失败：{e}"))?;
    }
    if cache_dir.exists() {
        let _ = std::fs::remove_dir_all(cache_dir);
    }
    let dst_str = cache_dir.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--depth", "1", "--single-branch"];
    if !branch.trim().is_empty() {
        args.push("--branch");
        args.push(branch.trim());
    }
    args.push("--");
    args.push(clone_url);
    args.push(&dst_str);
    run_git(&args)?;
    let _ = std::fs::write(cache_dir.join(FETCH_STAMP_FILE), b"ok");
    Ok(git_head_info(cache_dir))
}

fn truncate_preview(text: &str) -> String {
    if text.len() <= MAX_PREVIEW_BYTES {
        return text.to_string();
    }
    let mut end = MAX_PREVIEW_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n<!-- preview truncated -->", &text[..end])
}

fn is_script_path(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.starts_with("scripts/")
        || lower.ends_with(".sh")
        || lower.ends_with(".py")
        || lower.ends_with(".js")
        || lower.ends_with(".ts")
        || lower.ends_with(".ps1")
}

fn safe_repo_subpath(root: &Path, subpath: &str) -> Option<PathBuf> {
    let trimmed = subpath.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Some(root.to_path_buf());
    }
    let rel = Path::new(trimmed);
    if rel.is_absolute()
        || rel.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let joined = root.join(rel);
    if canonical_or_normalized(&joined).starts_with(canonical_or_normalized(root)) {
        Some(joined)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn scan_cached_repo(
    repo_id: &str,
    repo_label: &str,
    repo_url: &str,
    branch: &str,
    commit: &str,
    cache_dir: &Path,
    subpath: &str,
    skill_filter: Option<&str>,
    ssot_root: &Path,
) -> Vec<RemoteSkillCandidate> {
    let Some(search_root) = safe_repo_subpath(cache_dir, subpath) else {
        return Vec::new();
    };
    if !search_root.is_dir() {
        return Vec::new();
    }

    let filter_lower = skill_filter.map(|s| s.trim().to_ascii_lowercase());
    let mut by_subpath: BTreeMap<String, RemoteSkillCandidate> = BTreeMap::new();
    for entry in WalkDir::new(&search_root)
        .min_depth(0)
        .max_depth(6)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | ".github" | "node_modules")
        })
        .flatten()
    {
        if !entry.file_type().is_dir() {
            continue;
        }
        let dir = entry.path();
        let md_path = dir.join("SKILL.md");
        if !md_path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&md_path) else {
            continue;
        };
        let (fm_name, description) = parse_frontmatter(&content);
        let rel_sub = dir
            .strip_prefix(cache_dir)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let fallback_slug = if rel_sub.is_empty() {
            repo_id.rsplit('/').next().unwrap_or(repo_id).to_string()
        } else {
            dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("skill")
                .to_string()
        };
        let id = sanitize_skill_id(&fallback_slug).unwrap_or_else(|_| "skill".to_string());
        let name = if fm_name.is_empty() { id.clone() } else { fm_name };
        if let Some(ref flt) = filter_lower {
            if !id.to_ascii_lowercase().contains(flt) && !name.to_ascii_lowercase().contains(flt) {
                continue;
            }
        }
        let extra_files: Vec<String> = collect_extra_files(dir)
            .into_iter()
            .filter(|f| f.rel_path != FETCH_STAMP_FILE)
            .map(|f| f.rel_path)
            .collect();
        let has_scripts = extra_files.iter().any(|p| is_script_path(p));
        let in_library = ssot_root.join(&id).join("SKILL.md").is_file();

        by_subpath.insert(
            rel_sub.clone(),
            RemoteSkillCandidate {
                id,
                name,
                description,
                repo_id: repo_id.to_string(),
                repo_label: repo_label.to_string(),
                repo_url: repo_url.to_string(),
                branch: branch.to_string(),
                subpath: rel_sub,
                commit: commit.to_string(),
                source_kind: "git".to_string(),
                cache_dir: dir.to_string_lossy().into_owned(),
                skill_md_preview: truncate_preview(&content),
                extra_files,
                has_scripts,
                in_library,
            },
        );
    }
    by_subpath.into_values().collect()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverOutcome {
    pub candidates: Vec<RemoteSkillCandidate>,
    pub warnings: Vec<String>,
}

pub fn discover_remote_skills(
    app_data: &Path,
    cache_root: &Path,
    ssot_root: &Path,
    force_refresh: bool,
) -> DiscoverOutcome {
    let repos = load_repos(app_data);
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();

    for repo in repos {
        if !repo.enabled {
            continue;
        }
        let cache_dir = repo_cache_dir(cache_root, &repo.url, &repo.branch);
        match git_sync_repo(&repo.url, &repo.branch, &cache_dir, force_refresh) {
            Ok((commit, actual_branch)) => {
                let eff_branch = if repo.branch.is_empty() {
                    actual_branch
                } else {
                    repo.branch.clone()
                };
                let found = scan_cached_repo(
                    &repo.id,
                    &repo.label,
                    &repo.url,
                    &eff_branch,
                    &commit,
                    &cache_dir,
                    "",
                    None,
                    ssot_root,
                );
                candidates.extend(found);
            }
            Err(err) => {
                warnings.push(format!("{}：{err}", repo.id));
            }
        }
    }

    DiscoverOutcome {
        candidates,
        warnings,
    }
}

async fn download_markdown(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("agent-session-manager/0.2")
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败：{e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}：{}", resp.status(), url));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("读取响应失败：{e}"))?;
    if bytes.len() > MAX_DIRECT_URL_BYTES {
        return Err("文件超过 1 MB 大小限制".to_string());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "目标文件不是有效的 UTF-8 Markdown".to_string())
}

pub async fn fetch_remote_url(
    cache_root: &Path,
    ssot_root: &Path,
    input: &str,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    match parse_remote_input(input)? {
        ParsedRemoteInput::DirectMarkdownUrl { url, suggested_id } => {
            let content = download_markdown(&url).await?;
            let (fm_name, description) = parse_frontmatter(&content);
            let raw_id = suggested_id
                .or_else(|| if fm_name.is_empty() { None } else { Some(fm_name.clone()) })
                .unwrap_or_else(|| "remote-skill".to_string());
            let id = sanitize_skill_id(&raw_id)?;
            let name = if fm_name.is_empty() { id.clone() } else { fm_name };
            let in_library = ssot_root.join(&id).join("SKILL.md").is_file();
            Ok(vec![RemoteSkillCandidate {
                id,
                name,
                description,
                repo_id: url.clone(),
                repo_label: "Direct URL".to_string(),
                repo_url: url,
                branch: String::new(),
                subpath: String::new(),
                commit: String::new(),
                source_kind: "url".to_string(),
                cache_dir: String::new(),
                skill_md_preview: truncate_preview(&content),
                extra_files: Vec::new(),
                has_scripts: false,
                in_library,
            }])
        }
        ParsedRemoteInput::GitRepo {
            repo_id,
            clone_url,
            branch,
            subpath,
            skill_filter,
        } => {
            let cache_root_owned = cache_root.to_path_buf();
            let ssot_root_owned = ssot_root.to_path_buf();
            tokio::task::spawn_blocking(move || {
                let cache_dir = cache_root_owned
                    .join("skill-adhoc")
                    .join(cache_key(&clone_url, &branch));
                let (commit, actual_branch) =
                    git_sync_repo(&clone_url, &branch, &cache_dir, true)?;
                let eff_branch = if branch.is_empty() { actual_branch } else { branch };
                let found = scan_cached_repo(
                    &repo_id,
                    &repo_id,
                    &clone_url,
                    &eff_branch,
                    &commit,
                    &cache_dir,
                    &subpath,
                    skill_filter.as_deref(),
                    &ssot_root_owned,
                );
                if found.is_empty() {
                    return Err("在该仓库或路径中未找到包含 SKILL.md 的目录".to_string());
                }
                Ok(found)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
}

pub fn install_remote_skill(
    ssot_root: &Path,
    cache_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
    candidate: &RemoteSkillCandidate,
    target_agents: &[String],
) -> Result<(SkillEntry, SkillSyncOutcome), String> {
    let id = sanitize_skill_id(&candidate.id)?;
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

    if candidate.source_kind == "url" {
        let trimmed = candidate.skill_md_preview.trim_end_matches(['\n', '\r']);
        let body = if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}\n")
        };
        atomic_write(&dst.join("SKILL.md"), body.as_bytes(), 0o644)
            .map_err(|e| e.message())?;
    } else {
        let src = PathBuf::from(&candidate.cache_dir);
        let canon_src = canonical_or_normalized(&src);
        let canon_cache = canonical_or_normalized(cache_root);
        if !canon_src.starts_with(&canon_cache) || !src.join("SKILL.md").is_file() {
            return Err(format!("远程缓存目录无效或已失效：{}", src.display()));
        }
        copy_dir_all(&src, &dst)?;
    }

    let meta = SkillSourceMeta {
        kind: candidate.source_kind.clone(),
        repo: candidate.repo_id.clone(),
        branch: candidate.branch.clone(),
        subpath: candidate.subpath.clone(),
        url: candidate.repo_url.clone(),
        commit: candidate.commit.clone(),
        fetched_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    };
    write_source_meta(&dst, &meta)?;

    for raw in target_agents {
        if !existing_links.contains(raw) {
            existing_links.push(raw.clone());
        }
    }
    let (entry, mut outcome) =
        skills::sync_skill_links(ssot_root, home, env, backups_root, &id, &existing_links)?;
    outcome.backups.extend(extra_backups);
    if !outcome.backups.is_empty() {
        let _ = prune_backups(backups_root, 20);
    }
    Ok((entry, outcome))
}

pub async fn update_remote_skill(
    ssot_root: &Path,
    cache_root: &Path,
    home: &Path,
    env: &HashMap<String, String>,
    backups_root: &Path,
    raw_id: &str,
) -> Result<SkillUpdateOutcome, String> {
    let id = sanitize_skill_id(raw_id)?;
    let skill_dir = ssot_root.join(&id);
    let meta = read_source_meta(&skill_dir)
        .ok_or_else(|| format!("Skill「{id}」不是从网络拉取的，无法自动更新"))?;
    let previous_commit = meta.commit.clone();
    let old_md = std::fs::read(skill_dir.join("SKILL.md")).unwrap_or_default();
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let fetched_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

    if meta.kind == "url" {
        let downloaded = download_markdown(&meta.url).await?;
        let trimmed = downloaded.trim_end_matches(['\n', '\r']);
        let normalized = if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}\n")
        };
        if normalized.as_bytes() == old_md.as_slice() {
            let entry = read_skill_entry(ssot_root, &id, home, env)
                .ok_or_else(|| format!("读取 Skill「{id}」失败"))?;
            return Ok(SkillUpdateOutcome {
                entry,
                updated: false,
                previous_commit: previous_commit.clone(),
                current_commit: previous_commit,
                backup_path: None,
            });
        }
        let backup_dst = backups_root.join(&stamp).join("skills").join("library").join(&id);
        backup_existing_entry(&skill_dir, &backup_dst)?;
        atomic_write(&skill_dir.join("SKILL.md"), normalized.as_bytes(), 0o644)
            .map_err(|e| e.message())?;
        let mut next_meta = meta;
        next_meta.fetched_at = fetched_at;
        write_source_meta(&skill_dir, &next_meta)?;
        prune_backups(backups_root, 20).map_err(|e| e.message())?;
        let entry = read_skill_entry(ssot_root, &id, home, env)
            .ok_or_else(|| format!("读取 Skill「{id}」失败"))?;
        return Ok(SkillUpdateOutcome {
            entry,
            updated: true,
            previous_commit,
            current_commit: String::new(),
            backup_path: Some(backup_dst.to_string_lossy().into_owned()),
        });
    }

    let cache_root_owned = cache_root.to_path_buf();
    let meta_url = meta.url.clone();
    let meta_branch = meta.branch.clone();
    let meta_subpath = meta.subpath.clone();
    let (commit, actual_branch, src_dir) = tokio::task::spawn_blocking(move || {
        let cache_dir = repo_cache_dir(&cache_root_owned, &meta_url, &meta_branch);
        let (commit, actual_branch) = git_sync_repo(&meta_url, &meta_branch, &cache_dir, true)?;
        let src_dir = safe_repo_subpath(&cache_dir, &meta_subpath)
            .ok_or_else(|| format!("非法上游子路径：{meta_subpath}"))?;
        if !src_dir.join("SKILL.md").is_file() {
            return Err(format!("上游路径中已找不到 SKILL.md：{}", src_dir.display()));
        }
        Ok::<_, String>((commit, actual_branch, src_dir))
    })
    .await
    .map_err(|e| e.to_string())??;

    let new_md = std::fs::read(src_dir.join("SKILL.md"))
        .map_err(|e| format!("读取上游 SKILL.md 失败：{e}"))?;
    if !commit.is_empty() && commit == meta.commit && new_md == old_md {
        let entry = read_skill_entry(ssot_root, &id, home, env)
            .ok_or_else(|| format!("读取 Skill「{id}」失败"))?;
        return Ok(SkillUpdateOutcome {
            entry,
            updated: false,
            previous_commit,
            current_commit: commit,
            backup_path: None,
        });
    }

    let backup_dst = backups_root.join(&stamp).join("skills").join("library").join(&id);
    backup_existing_entry(&skill_dir, &backup_dst)?;
    copy_dir_all(&src_dir, &skill_dir)?;
    let mut next_meta = meta;
    next_meta.commit = commit.clone();
    if next_meta.branch.is_empty() {
        next_meta.branch = actual_branch;
    }
    next_meta.fetched_at = fetched_at;
    write_source_meta(&skill_dir, &next_meta)?;
    prune_backups(backups_root, 20).map_err(|e| e.message())?;
    let entry = read_skill_entry(ssot_root, &id, home, env)
        .ok_or_else(|| format!("读取 Skill「{id}」失败"))?;
    Ok(SkillUpdateOutcome {
        entry,
        updated: true,
        previous_commit,
        current_commit: commit,
        backup_path: Some(backup_dst.to_string_lossy().into_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::models::AgentKind;
    use crate::config::skills::agent_skills_dir;
    use std::time::UNIX_EPOCH;

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "asm_skills_remote_{}_{}",
                name,
                SystemTime::now()
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
    fn parse_remote_input_handles_all_formats() {
        assert_eq!(
            parse_remote_input("anthropics/skills").unwrap(),
            ParsedRemoteInput::GitRepo {
                repo_id: "anthropics/skills".into(),
                clone_url: "https://github.com/anthropics/skills.git".into(),
                branch: String::new(),
                subpath: String::new(),
                skill_filter: None,
            }
        );

        assert_eq!(
            parse_remote_input("npx skills add vercel-labs/agent-skills --skill web-design -g -y").unwrap(),
            ParsedRemoteInput::GitRepo {
                repo_id: "vercel-labs/agent-skills".into(),
                clone_url: "https://github.com/vercel-labs/agent-skills.git".into(),
                branch: String::new(),
                subpath: String::new(),
                skill_filter: Some("web-design".into()),
            }
        );

        assert_eq!(
            parse_remote_input("https://github.com/anthropics/skills/tree/main/skills/pdf").unwrap(),
            ParsedRemoteInput::GitRepo {
                repo_id: "anthropics/skills".into(),
                clone_url: "https://github.com/anthropics/skills.git".into(),
                branch: "main".into(),
                subpath: "skills/pdf".into(),
                skill_filter: None,
            }
        );

        assert_eq!(
            parse_remote_input("https://raw.githubusercontent.com/org/repo/main/skills/review/SKILL.md").unwrap(),
            ParsedRemoteInput::DirectMarkdownUrl {
                url: "https://raw.githubusercontent.com/org/repo/main/skills/review/SKILL.md".into(),
                suggested_id: Some("review".into()),
            }
        );

        assert!(parse_remote_input("--upload-pack=evil").is_err());
        assert!(parse_remote_input("file:///etc/passwd").is_err());
    }

    #[tokio::test]
    async fn git_install_and_update_from_local_repo() {
        let dir = TestDir::new("git_update");
        let root = dir.path();
        let origin = root.join("origin");
        let ssot = root.join("app_data/skills");
        let cache = root.join("app_cache");
        let home = root.join("home");
        let backups = root.join("app_data/backups");
        let env = HashMap::new();

        let skill_src = origin.join("skills/helper");
        std::fs::create_dir_all(skill_src.join("scripts")).unwrap();
        std::fs::write(
            skill_src.join("SKILL.md"),
            "---\nname: Helper\ndescription: V1\n---\n\n# V1\n",
        )
        .unwrap();
        std::fs::write(skill_src.join("scripts/run.sh"), "#!/bin/sh\necho v1\n").unwrap();

        let o = origin.to_string_lossy().into_owned();
        run_git(&["init", &o]).unwrap();
        run_git(&["-C", &o, "config", "user.name", "Test"]).unwrap();
        run_git(&["-C", &o, "config", "user.email", "test@example.com"]).unwrap();
        run_git(&["-C", &o, "add", "."]).unwrap();
        run_git(&["-C", &o, "commit", "-m", "v1"]).unwrap();

        let clone_url = format!("file://{o}");
        let cached = repo_cache_dir(&cache, &clone_url, "");
        let (c1, branch) = git_sync_repo(&clone_url, "", &cached, true).unwrap();
        let candidates = scan_cached_repo(
            "local/test",
            "Local Test",
            &clone_url,
            &branch,
            &c1,
            &cached,
            "",
            None,
            &ssot,
        );
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].has_scripts);

        let (installed, _) = install_remote_skill(
            &ssot,
            &cache,
            &home,
            &env,
            &backups,
            &candidates[0],
            &["antigravity".into()],
        )
        .unwrap();
        assert_eq!(installed.linked_agents, vec!["antigravity".to_string()]);
        assert_eq!(installed.source.as_ref().unwrap().commit, c1);

        // Commit V2 upstream
        std::fs::write(
            skill_src.join("SKILL.md"),
            "---\nname: Helper\ndescription: V2\n---\n\n# V2\n",
        )
        .unwrap();
        run_git(&["-C", &o, "add", "."]).unwrap();
        run_git(&["-C", &o, "commit", "-m", "v2"]).unwrap();

        let updated = update_remote_skill(&ssot, &cache, &home, &env, &backups, "helper")
            .await
            .unwrap();
        assert!(updated.updated);
        assert_ne!(updated.current_commit, c1);
        assert!(updated.backup_path.is_some());

        let agy_md = agent_skills_dir(AgentKind::Antigravity, &home, &env).join("helper/SKILL.md");
        assert!(std::fs::read_to_string(agy_md).unwrap().contains("# V2"));
    }
}
