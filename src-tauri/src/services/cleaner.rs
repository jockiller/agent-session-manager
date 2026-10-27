use std::collections::HashSet;
use std::path::Path;

use crate::adapters::AgentAdapter;
use crate::models::{CleanupPlan, CleanupResult, SessionSummary};

/// Ids of every descendant (child, grandchild, ...) of `root` within `all_sessions`.
fn descendant_ids(all_sessions: &[SessionSummary], root: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut frontier: Vec<&str> = vec![root];
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(current) = frontier.pop() {
        for s in all_sessions {
            if s.parent_id.as_deref() == Some(current) && seen.insert(s.id.as_str()) {
                ids.push(s.id.clone());
                frontier.push(s.id.as_str());
            }
        }
    }
    ids
}

fn running_ids(all_sessions: &[SessionSummary]) -> HashSet<&str> {
    all_sessions
        .iter()
        .filter(|s| s.is_running)
        .map(|s| s.id.as_str())
        .collect()
}

/// Platforms whose adapter deletes a session together with its child sessions.
fn cascading_platforms(adapters: &[Box<dyn AgentAdapter>]) -> HashSet<String> {
    adapters
        .iter()
        .filter(|a| a.deletes_descendants())
        .map(|a| a.platform_id().to_string())
        .collect()
}

/// Whether a target session must be left alone: it is still running, or its platform removes the
/// child sessions together with it and one of those children is still running.
fn is_protected(
    session: &SessionSummary,
    all_sessions: &[SessionSummary],
    running: &HashSet<&str>,
    cascading: &HashSet<String>,
    force: bool,
) -> bool {
    if force {
        return false;
    }
    if session.is_running {
        return true;
    }
    if !cascading.contains(session.platform.as_str()) {
        return false;
    }
    descendant_ids(all_sessions, &session.id)
        .iter()
        .any(|id| running.contains(id.as_str()))
}

pub fn plan_cleanup(
    all_sessions: &[SessionSummary],
    target_ids: &[String],
    force: bool,
    adapters: &[Box<dyn AgentAdapter>],
) -> CleanupPlan {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let running = running_ids(all_sessions);
    let cascading = cascading_platforms(adapters);
    let mut deletable = 0;
    let mut skipped = 0;
    let mut bytes_to_free = 0u64;
    let mut file_count = 0;

    for s in all_sessions {
        if target_set.contains(s.id.as_str()) {
            if is_protected(s, all_sessions, &running, &cascading, force) {
                skipped += 1;
            } else {
                deletable += 1;
                bytes_to_free += s.size_bytes;
                file_count += s.all_paths.len();
            }
        }
    }

    CleanupPlan {
        target_ids: target_ids.to_vec(),
        deletable_count: deletable,
        skipped_running_count: skipped,
        total_bytes_to_free: bytes_to_free,
        associated_files_count: file_count,
    }
}

pub fn execute_cleanup(
    all_sessions: &[SessionSummary],
    target_ids: &[String],
    force: bool,
    adapters: &[Box<dyn AgentAdapter>],
) -> CleanupResult {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let running = running_ids(all_sessions);
    let cascading = cascading_platforms(adapters);
    let mut result = CleanupResult {
        success: true,
        trash_method: "Native OS Trash".to_string(),
        ..Default::default()
    };

    let mut removed_by_platform: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for s in all_sessions {
        if !target_set.contains(s.id.as_str()) {
            continue;
        }

        if is_protected(s, all_sessions, &running, &cascading, force) {
            continue;
        }

        let mut all_ok = true;
        for path_str in &s.all_paths {
            let p = Path::new(path_str);
            if p.exists() {
                // Safely move to native trash using in-process NSFileManager (never spawns osascript/Dock icons)
                let mut ctx = trash::TrashContext::default();
                #[cfg(target_os = "macos")]
                {
                    use trash::macos::{DeleteMethod, TrashContextExtMacos};
                    ctx.set_delete_method(DeleteMethod::NsFileManager);
                }

                if let Err(err) = ctx.delete(p) {
                    // Fallback to moving directly into ~/.Trash
                    let mut fallback_ok = false;
                    #[cfg(target_os = "macos")]
                    {
                        if let Some(home) = crate::adapters::user_home() {
                            let trash_dir = home.join(".Trash");
                            if trash_dir.exists() {
                                let name = p.file_name().unwrap_or_default().to_string_lossy();
                                let dest = trash_dir.join(format!("{}_{}", name, chrono::Local::now().timestamp_millis()));
                                if std::fs::rename(p, &dest).is_ok() {
                                    fallback_ok = true;
                                }
                            }
                        }
                    }
                    #[cfg(target_os = "linux")]
                    {
                        if let Some(home) = crate::adapters::user_home() {
                            let trash_dir = home.join(".local").join("share").join("Trash").join("files");
                            if trash_dir.exists() {
                                let name = p.file_name().unwrap_or_default().to_string_lossy();
                                let dest = trash_dir.join(format!("{}_{}", name, chrono::Local::now().timestamp_millis()));
                                if std::fs::rename(p, &dest).is_ok() {
                                    fallback_ok = true;
                                }
                            }
                        }
                    }

                    if !fallback_ok {
                        all_ok = false;
                        result.errors.push(format!("Failed to trash {}: {}", path_str, err));
                    }
                }
            }
        }

        if all_ok {
            result.removed_sessions += 1;
            result.freed_bytes += s.size_bytes;
            removed_by_platform.entry(s.platform.clone()).or_default().push(s.id.clone());
        } else {
            result.failed_sessions += 1;
        }
    }

    // Prune indexes for removed sessions across adapters
    for (platform, ids) in removed_by_platform {
        for adapter in adapters {
            if adapter.platform_id() == platform {
                let pruned = adapter.prune_indexes(&ids);
                result.indexes_pruned += pruned;

                // Store-backed agents (e.g. OpenCode) keep no files on disk, so the store rows are
                // the only proof a session is gone. Report a failed deletion instead of claiming a
                // success the agent store did not perform.
                if adapter.store_backed() && pruned < ids.len() {
                    let failed = ids.len() - pruned;
                    result.removed_sessions = result.removed_sessions.saturating_sub(failed);
                    result.failed_sessions += failed;
                    result.errors.push(format!(
                        "{}: {} session(s) are still present in the agent store after cleanup",
                        platform, failed
                    ));
                }
            }
        }
    }

    if result.failed_sessions > 0 {
        result.success = false;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChatMessage;

    struct FakeAdapter {
        platform: &'static str,
        cascading: bool,
        fail_store: bool,
        seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl AgentAdapter for FakeAdapter {
        fn platform_id(&self) -> &'static str {
            self.platform
        }
        fn display_name(&self) -> &'static str {
            "Fake"
        }
        fn scan_sessions(&self) -> Vec<SessionSummary> {
            Vec::new()
        }
        fn load_messages(&self, _session: &SessionSummary, _limit: usize) -> Vec<ChatMessage> {
            Vec::new()
        }
        fn prune_indexes(&self, session_ids: &[String]) -> usize {
            self.seen.lock().unwrap().extend(session_ids.iter().cloned());
            if self.fail_store {
                0
            } else {
                session_ids.len()
            }
        }
        fn store_backed(&self) -> bool {
            true
        }
        fn deletes_descendants(&self) -> bool {
            self.cascading
        }
    }

    /// Adapter stand-in plus the ids it was asked to prune.
    fn fake(
        cascading: bool,
        fail_store: bool,
    ) -> (
        Box<dyn AgentAdapter>,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let adapter = FakeAdapter {
            platform: "fake",
            cascading,
            fail_store,
            seen: seen.clone(),
        };
        (Box::new(adapter), seen)
    }

    fn session(id: &str, parent: Option<&str>, running: bool) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            platform: "fake".to_string(),
            flavor: "fake".to_string(),
            dirname: id.to_string(),
            main_path: String::new(),
            all_paths: Vec::new(),
            cwd: String::new(),
            title: id.to_string(),
            first_prompt: String::new(),
            created_at: 0,
            updated_at: 0,
            size_bytes: 4096,
            turn_count: 1,
            is_subagent: parent.is_some(),
            parent_id: parent.map(|p| p.to_string()),
            is_running: running,
            is_archived: false,
            has_transcript: true,
        }
    }

    #[test]
    fn test_descendant_ids_walks_the_tree() {
        let sessions = vec![
            session("root", None, false),
            session("child_a", Some("root"), false),
            session("child_b", Some("root"), false),
            session("grandchild", Some("child_a"), false),
            session("unrelated", None, false),
        ];
        let mut found = descendant_ids(&sessions, "root");
        found.sort();
        assert_eq!(found, vec!["child_a", "child_b", "grandchild"]);

        let mut found = descendant_ids(&sessions, "child_a");
        found.sort();
        assert_eq!(found, vec!["grandchild"]);
        assert!(descendant_ids(&sessions, "grandchild").is_empty());
    }

    #[test]
    fn test_parent_with_running_child_is_protected() {
        let sessions = vec![
            session("parent", None, false),
            session("child", Some("parent"), true),
        ];
        let running = running_ids(&sessions);
        let cascading: HashSet<String> = ["fake".to_string()].into_iter().collect();

        // A cascading delete of the parent would take the running child with it.
        assert!(is_protected(&sessions[0], &sessions, &running, &cascading, false));
        // The user can still force it, and the running session itself is always protected.
        assert!(!is_protected(&sessions[0], &sessions, &running, &cascading, true));
        assert!(is_protected(&sessions[1], &sessions, &running, &cascading, false));
    }

    #[test]
    fn test_parent_stays_deletable_when_adapter_keeps_children() {
        let sessions = vec![
            session("parent", None, false),
            session("child", Some("parent"), true),
        ];
        let running = running_ids(&sessions);
        let non_cascading: HashSet<String> = HashSet::new();
        assert!(!is_protected(
            &sessions[0],
            &sessions,
            &running,
            &non_cascading,
            false
        ));
    }

    #[test]
    fn test_execute_cleanup_skips_running_family_without_force() {
        let sessions = vec![
            session("parent", None, false),
            session("child", Some("parent"), true),
        ];
        let targets = ["parent".to_string(), "child".to_string()];

        // Without force the running child protects its parent too, so the store is never touched.
        let (adapter, seen) = fake(true, false);
        let adapters: Vec<Box<dyn AgentAdapter>> = vec![adapter];
        let result = execute_cleanup(&sessions, &targets, false, &adapters);
        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(result.removed_sessions, 0);
        assert_eq!(result.failed_sessions, 0);
        assert!(result.success);

        // Force overrides the protection and both sessions go.
        let (adapter, seen) = fake(true, false);
        let adapters: Vec<Box<dyn AgentAdapter>> = vec![adapter];
        let result = execute_cleanup(&sessions, &targets, true, &adapters);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(result.removed_sessions, 2);
        assert!(result.success);
    }

    #[test]
    fn test_execute_cleanup_reports_store_deletion_failure() {
        let sessions = vec![session("gone_nowhere", None, false)];
        // The store reports nothing pruned, e.g. because the write was rejected.
        let (adapter, _) = fake(false, true);
        let adapters: Vec<Box<dyn AgentAdapter>> = vec![adapter];

        let result = execute_cleanup(&sessions, &["gone_nowhere".to_string()], true, &adapters);

        assert!(!result.success);
        assert_eq!(result.removed_sessions, 0);
        assert_eq!(result.failed_sessions, 1);
        assert_eq!(result.errors.len(), 1);
    }
}
