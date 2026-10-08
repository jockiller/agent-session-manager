use std::collections::HashSet;
use std::path::Path;

use crate::adapters::AgentAdapter;
use crate::models::{CleanupPlan, CleanupResult, SessionSummary};

pub fn plan_cleanup(
    all_sessions: &[SessionSummary],
    target_ids: &[String],
    force: bool,
) -> CleanupPlan {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let mut deletable = 0;
    let mut skipped = 0;
    let mut bytes_to_free = 0u64;
    let mut file_count = 0;

    for s in all_sessions {
        if target_set.contains(s.id.as_str()) {
            if s.is_running && !force {
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

        if s.is_running && !force {
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
                result.indexes_pruned += adapter.prune_indexes(&ids);
            }
        }
    }

    result
}
