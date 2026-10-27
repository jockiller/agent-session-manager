export interface SessionSummary {
  id: string;
  platform: string;
  flavor: string;
  dirname: string;
  main_path: string;
  all_paths: string[];
  cwd: string;
  title: string;
  first_prompt?: string;
  created_at: number;
  updated_at: number;
  size_bytes: number;
  turn_count: number;
  is_subagent: boolean;
  parent_id?: string | null;
  is_running: boolean;
  is_archived: boolean;
  has_transcript: boolean;
}

export interface SessionTreeNode {
  session: SessionSummary;
  subagents: SessionSummary[];
}

export interface ToolCallItem {
  name: string;
  args?: string | null;
  output?: string | null;
}

export interface ChatMessage {
  role: "user" | "assistant" | "subagent" | "tool" | "system";
  text: string;
  time: string;
  msg_type: string;
  thinking?: string | null;
  tool_calls: ToolCallItem[];
}

export interface PlatformStat {
  count: number;
  bytes: number;
}

export interface CleanupPlan {
  target_ids: string[];
  deletable_count: number;
  skipped_running_count: number;
  total_bytes_to_free: number;
  associated_files_count: number;
}

export interface CleanupResult {
  success: boolean;
  removed_sessions: number;
  failed_sessions: number;
  freed_bytes: number;
  indexes_pruned: number;
  trash_method: string;
  errors: string[];
}

export interface GlobalStats {
  total_sessions: number;
  main_sessions: number;
  subagents: number;
  total_bytes: number;
  running_count: number;
  archived_count: number;
  orphan_count: number;
  platform_distribution: Record<string, PlatformStat>;
}
