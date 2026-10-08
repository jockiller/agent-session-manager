use std::collections::HashMap;
use std::process::Command;

use crate::adapters::antigravity::AntigravityAdapter;
use crate::adapters::claude::ClaudeAdapter;
use crate::adapters::codebuddy::CodeBuddyAdapter;
use crate::adapters::codex::CodexAdapter;
use crate::adapters::cursor::CursorAdapter;
use crate::adapters::deepseek::DeepSeekAdapter;
use crate::adapters::dsh::DshAdapter;
use crate::adapters::gemini::GeminiAdapter;
use crate::adapters::grok::GrokAdapter;
use crate::adapters::hermes::HermesAdapter;
use crate::adapters::kimi::KimiAdapter;
use crate::adapters::openclaw::OpenClawAdapter;
use crate::adapters::openclaude::OpenClaudeAdapter;
use crate::adapters::opencode::OpenCodeAdapter;
use crate::adapters::pi::PiAdapter;
use crate::adapters::qoder::QoderAdapter;
use crate::adapters::qwen::QwenAdapter;
use crate::adapters::trae::TraeAdapter;
use crate::adapters::workbuddy::WorkBuddyAdapter;
use crate::adapters::zcode::ZCodeAdapter;
use crate::adapters::AgentAdapter;
use crate::models::{ChatMessage, CleanupPlan, CleanupResult, GlobalStats, PlatformStat, SessionSummary};
use crate::services::cleaner::{execute_cleanup as svc_execute_cleanup, plan_cleanup as svc_plan_cleanup};

fn get_all_adapters() -> Vec<Box<dyn AgentAdapter>> {
    vec![
        Box::new(AntigravityAdapter),
        Box::new(ClaudeAdapter),
        Box::new(CodeBuddyAdapter),
        Box::new(CodexAdapter),
        Box::new(CursorAdapter),
        Box::new(DeepSeekAdapter),
        Box::new(DshAdapter),
        Box::new(GeminiAdapter),
        Box::new(GrokAdapter),
        Box::new(HermesAdapter),
        Box::new(KimiAdapter),
        Box::new(OpenClawAdapter),
        Box::new(OpenClaudeAdapter),
        Box::new(OpenCodeAdapter::new_opencode()),
        Box::new(OpenCodeAdapter::new_codewiz()),
        Box::new(PiAdapter::new_pi()),
        Box::new(PiAdapter::new_omp()),
        Box::new(QoderAdapter),
        Box::new(QwenAdapter),
        Box::new(TraeAdapter),
        Box::new(WorkBuddyAdapter),
        Box::new(ZCodeAdapter::new()),
    ]
}

use rayon::prelude::*;

#[tauri::command]
pub fn scan_all_sessions() -> Vec<SessionSummary> {
    let adapters = get_all_adapters();
    let mut all: Vec<SessionSummary> = adapters
        .into_par_iter()
        .flat_map(|adapter| adapter.scan_sessions())
        .collect();

    // Sort by updated_at descending
    all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    all
}

#[tauri::command]
pub fn get_session_messages(session: SessionSummary, max_msgs: Option<usize>) -> Vec<ChatMessage> {
    let limit = max_msgs.unwrap_or(150);
    let adapters = get_all_adapters();
    for adapter in adapters {
        if adapter.platform_id() == session.platform {
            return adapter.load_messages(&session, limit);
        }
    }
    Vec::new()
}

#[tauri::command]
pub fn plan_cleanup_cmd(
    all_sessions: Vec<SessionSummary>,
    target_ids: Vec<String>,
    force: Option<bool>,
) -> CleanupPlan {
    svc_plan_cleanup(&all_sessions, &target_ids, force.unwrap_or(false))
}

#[tauri::command]
pub fn execute_cleanup_cmd(
    all_sessions: Vec<SessionSummary>,
    target_ids: Vec<String>,
    force: Option<bool>,
) -> CleanupResult {
    let adapters = get_all_adapters();
    svc_execute_cleanup(&all_sessions, &target_ids, force.unwrap_or(false), &adapters)
}

#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        let win_path = path.replace('/', "\\");
        Command::new("explorer")
            .arg(format!("/select,{}", win_path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let target = if std::path::Path::new(&path).is_file() {
            std::path::Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(path)
        } else {
            path
        };
        Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_terminal(cwd: String) -> Result<(), String> {
    if cwd.is_empty() {
        return Err("Path is empty".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg("-a")
            .arg("Terminal")
            .arg(&cwd)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "", "cmd.exe"])
            .current_dir(&cwd)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "alacritty",
            "kitty",
            "xterm",
        ];
        let mut spawned = false;
        for term in candidates {
            if Command::new(term).current_dir(&cwd).spawn().is_ok() {
                spawned = true;
                break;
            }
        }
        if !spawned {
            Command::new("xdg-open")
                .arg(&cwd)
                .spawn()
                .map_err(|e| format!("Failed to spawn terminal or open folder: {}", e))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn run_in_terminal(command: String) -> Result<(), String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("Command is empty".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        let escaped = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
        let apple_script = format!(
            "tell application \"Terminal\"\nactivate\ndo script \"{}\"\nend tell",
            escaped
        );
        Command::new("osascript")
            .arg("-e")
            .arg(&apple_script)
            .spawn()
            .map_err(|e| format!("Failed to spawn terminal: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "Agent Session", "cmd.exe", "/k", trimmed])
            .spawn()
            .map_err(|e| format!("Failed to spawn cmd: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        let bash_cmd = format!("{}; exec bash", trimmed);
        let candidates: [(&str, &[&str]); 7] = [
            ("x-terminal-emulator", &["-e", "bash", "-c", &bash_cmd]),
            ("gnome-terminal", &["--", "bash", "-c", &bash_cmd]),
            ("konsole", &["-e", "bash", "-c", &bash_cmd]),
            ("xfce4-terminal", &["-x", "bash", "-c", &bash_cmd]),
            ("alacritty", &["-e", "bash", "-c", &bash_cmd]),
            ("kitty", &["bash", "-c", &bash_cmd]),
            ("xterm", &["-e", "bash", "-c", &bash_cmd]),
        ];
        let mut spawned = false;
        for (term, args) in candidates {
            if Command::new(term).args(args).spawn().is_ok() {
                spawned = true;
                break;
            }
        }
        if !spawned {
            return Err("No supported Linux terminal emulator found (checked x-terminal-emulator, gnome-terminal, konsole, xfce4-terminal, alacritty, kitty, xterm)".to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn calculate_global_stats(sessions: Vec<SessionSummary>) -> GlobalStats {
    let mut stats = GlobalStats {
        total_sessions: sessions.len(),
        ..Default::default()
    };

    let mut plat_map: HashMap<String, PlatformStat> = HashMap::new();

    for s in sessions {
        stats.total_bytes += s.size_bytes;
        if s.is_subagent {
            stats.subagents += 1;
        } else {
            stats.main_sessions += 1;
        }
        if s.is_running {
            stats.running_count += 1;
        }
        if !s.has_transcript {
            stats.orphan_count += 1;
        }

        let mut s_tokens = 0u64;

        if let Some(ref ts) = s.token_stats {
            stats.total_prompt_tokens += ts.prompt_tokens;
            stats.total_completion_tokens += ts.completion_tokens;
            stats.total_cache_read_tokens += ts.cache_read_tokens;
            stats.total_cache_write_tokens += ts.cache_write_tokens;
            stats.total_tokens += ts.total_tokens;
            s_tokens = ts.total_tokens;
        }

        let entry = plat_map.entry(s.platform.clone()).or_default();
        entry.count += 1;
        entry.bytes += s.size_bytes;
        entry.tokens += s_tokens;
    }

    // Calculate overall cache hit rate
    let total_cache_prompt = stats.total_prompt_tokens + stats.total_cache_read_tokens;
    if total_cache_prompt > 0 && stats.total_cache_read_tokens > 0 {
        let rate = (stats.total_cache_read_tokens as f64 / total_cache_prompt as f64) * 100.0;
        stats.overall_cache_hit_rate = Some((rate * 10.0).round() / 10.0);
    }

    stats.platform_distribution = plat_map;
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_all_sessions() {
        let sessions = scan_all_sessions();
        assert!(!sessions.is_empty(), "Should scan at least one session on this machine");
        let stats = calculate_global_stats(sessions.clone());
        println!("Scanned {} sessions, {} subagents, total tokens: {}", 
            stats.total_sessions, stats.subagents, stats.total_tokens);
        for (plat, pstat) in &stats.platform_distribution {
            println!("  Platform {}: {} sessions, {} tokens", plat, pstat.count, pstat.tokens);
        }
        let zcode_flavors: std::collections::HashSet<_> = sessions.iter().filter(|s| s.platform == "zcode").map(|s| &s.flavor).collect();
        println!("  ZCode distinct flavors/models: {:?}", zcode_flavors);
        assert!(stats.total_sessions > 0);
        assert!(stats.total_bytes > 0);
    }
}
