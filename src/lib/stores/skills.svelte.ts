import { invoke } from "@tauri-apps/api/core";
import { t } from "$lib/i18n";
import { invokeError } from "$lib/config/document";
import { configStore } from "$lib/config/store.svelte";
import type {
  AgentKind,
  DiscoveredDiskSkill,
  RemoteSkillCandidate,
  SkillDiscoverOutcome,
  SkillEntry,
  SkillRepo,
  SkillSyncOutcome,
  SkillUpdateOutcome,
} from "$lib/config/types";

function sameStringSet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const sa = [...a].sort();
  const sb = [...b].sort();
  return sa.every((v, idx) => v === sb[idx]);
}

class SkillsStore {
  skills = $state<SkillEntry[]>([]);
  selectedID = $state<string | null>(null);
  draftContent = $state("");
  draftLinkedAgents = $state<string[]>([]);
  loading = $state(false);
  saving = $state(false);
  loaded = $state(false);
  repos = $state<SkillRepo[]>([]);
  remoteCandidates = $state<RemoteSkillCandidate[]>([]);
  remoteWarnings = $state<string[]>([]);
  discovering = $state(false);
  fetchingUrl = $state(false);
  updatingRemote = $state(false);

  selectedSkill = $derived.by(() => {
    const id = this.selectedID;
    if (!id) return null;
    return this.skills.find((s) => s.id === id) ?? null;
  });

  isDirty = $derived.by(() => {
    const sel = this.selectedSkill;
    if (!sel) return false;
    return (
      this.draftContent !== sel.content ||
      !sameStringSet(this.draftLinkedAgents, sel.linkedAgents)
    );
  });

  private syncDraftFrom(entry: SkillEntry | null): void {
    if (!entry) {
      this.draftContent = "";
      this.draftLinkedAgents = [];
      return;
    }
    this.draftContent = entry.content;
    this.draftLinkedAgents = [...entry.linkedAgents];
  }

  async ensureLoaded(): Promise<void> {
    if (this.loading || this.loaded) return;
    await this.load();
  }

  async load(): Promise<void> {
    this.loading = true;
    try {
      const list = await invoke<SkillEntry[]>("skills_list");
      this.skills = Array.isArray(list) ? list : [];
      if (!this.selectedID || !this.skills.some((s) => s.id === this.selectedID)) {
        this.selectedID = this.skills[0]?.id ?? null;
      }
      this.syncDraftFrom(this.selectedSkill);
      this.loaded = true;
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
    } finally {
      this.loading = false;
    }
  }

  select(id: string): void {
    this.selectedID = id;
    this.syncDraftFrom(this.skills.find((s) => s.id === id) ?? null);
  }

  revert(): void {
    this.syncDraftFrom(this.selectedSkill);
  }

  toggleDraftAgent(kind: AgentKind): void {
    if (this.draftLinkedAgents.includes(kind)) {
      this.draftLinkedAgents = this.draftLinkedAgents.filter((k) => k !== kind);
    } else {
      this.draftLinkedAgents = [...this.draftLinkedAgents, kind];
    }
  }

  private upsertSkill(entry: SkillEntry): void {
    const idx = this.skills.findIndex((s) => s.id === entry.id);
    if (idx >= 0) {
      const next = [...this.skills];
      next[idx] = entry;
      this.skills = next;
    } else {
      this.skills = [...this.skills, entry].sort((a, b) => a.id.localeCompare(b.id));
    }
    this.selectedID = entry.id;
    this.syncDraftFrom(entry);
  }

  async createSkill(id: string, name: string, description: string): Promise<boolean> {
    const slug = id.trim();
    const displayName = name.trim() || slug;
    const descText =
      description.trim() ||
      `Use this skill when working with ${displayName}.`;
    const body = `---\nname: ${displayName}\ndescription: ${descText}\n---\n\n# ${displayName}\n\n`;
    this.saving = true;
    try {
      const created = await invoke<SkillEntry>("skills_save", { id: slug, content: body });
      this.upsertSkill(created);
      configStore.show({
        title: t("cfg_saved"),
        detail: created.skillMdPath,
        isError: false,
        revealedPath: created.path,
      });
      return true;
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
      return false;
    } finally {
      this.saving = false;
    }
  }

  async saveAndSyncSelected(): Promise<void> {
    const sel = this.selectedSkill;
    if (!sel) return;
    this.saving = true;
    try {
      const saved = await invoke<SkillEntry>("skills_save", {
        id: sel.id,
        content: this.draftContent,
      });
      const [synced, outcome] = await invoke<[SkillEntry, SkillSyncOutcome]>(
        "skills_sync_links",
        {
          id: saved.id,
          targetAgents: this.draftLinkedAgents,
        },
      );
      this.upsertSkill(synced);
      const changedCount = outcome.linked.length + outcome.unlinked.length;
      if (changedCount > 0) {
        const lines = [...outcome.linked, ...outcome.unlinked];
        if (outcome.backups.length > 0) {
          lines.push(t("cfg_toast_backups", { n: outcome.backups.length }));
        }
        configStore.show({
          title: t("skills_sync_success", { n: changedCount }),
          detail: lines.join("\n"),
          isError: false,
          revealedPath: outcome.linked[0] ?? synced.path,
        });
      } else {
        configStore.show({
          title: t("skills_sync_noop"),
          detail: "",
          isError: false,
          revealedPath: synced.path,
        });
      }
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
    } finally {
      this.saving = false;
    }
  }

  async deleteSelected(): Promise<void> {
    const sel = this.selectedSkill;
    if (!sel) return;
    this.saving = true;
    try {
      await invoke<string[]>("skills_delete", { id: sel.id });
      const remaining = this.skills.filter((s) => s.id !== sel.id);
      this.skills = remaining;
      this.selectedID = remaining[0]?.id ?? null;
      this.syncDraftFrom(this.selectedSkill);
      configStore.show({
        title: t("skills_deleted", { id: sel.id }),
        detail: "",
        isError: false,
        revealedPath: "",
      });
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
    } finally {
      this.saving = false;
    }
  }

  async scanDisk(): Promise<DiscoveredDiskSkill[]> {
    return invoke<DiscoveredDiskSkill[]>("skills_scan_disk");
  }

  async importFromDisk(
    sourcePath: string,
    skillId: string | null,
    linkAgents: string[],
  ): Promise<SkillEntry | null> {
    this.saving = true;
    try {
      const [entry, outcome] = await invoke<[SkillEntry, SkillSyncOutcome]>(
        "skills_import_dir",
        {
          sourcePath,
          skillId,
          linkAgents,
        },
      );
      this.upsertSkill(entry);
      const detailLines = [entry.path, ...outcome.linked];
      configStore.show({
        title: t("cfg_saved"),
        detail: detailLines.join("\n"),
        isError: false,
        revealedPath: entry.path,
      });
      return entry;
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
      return null;
    } finally {
      this.saving = false;
    }
  }

  async loadRepos(): Promise<void> {
    const list = await invoke<SkillRepo[]>("skills_list_repos");
    this.repos = Array.isArray(list) ? list : [];
  }

  async saveRepos(repos: SkillRepo[]): Promise<void> {
    const saved = await invoke<SkillRepo[]>("skills_save_repos", { repos });
    this.repos = Array.isArray(saved) ? saved : [];
  }

  async discoverRemote(forceRefresh = false): Promise<void> {
    this.discovering = true;
    try {
      await this.loadRepos();
      const outcome = await invoke<SkillDiscoverOutcome>("skills_discover_remote", {
        forceRefresh,
      });
      this.remoteCandidates = Array.isArray(outcome?.candidates) ? outcome.candidates : [];
      this.remoteWarnings = Array.isArray(outcome?.warnings) ? outcome.warnings : [];
    } catch (err) {
      this.remoteWarnings = [invokeError(err)];
    } finally {
      this.discovering = false;
    }
  }

  async fetchRemoteUrl(input: string): Promise<RemoteSkillCandidate[]> {
    this.fetchingUrl = true;
    try {
      const list = await invoke<RemoteSkillCandidate[]>("skills_fetch_remote_url", { input });
      return Array.isArray(list) ? list : [];
    } finally {
      this.fetchingUrl = false;
    }
  }

  async installRemote(
    candidate: RemoteSkillCandidate,
    targetAgents: string[],
  ): Promise<SkillEntry | null> {
    this.saving = true;
    try {
      const [entry, outcome] = await invoke<[SkillEntry, SkillSyncOutcome]>(
        "skills_install_remote",
        {
          candidate,
          targetAgents,
        },
      );
      this.upsertSkill(entry);
      this.remoteCandidates = this.remoteCandidates.map((item) =>
        item.id === entry.id ? { ...item, inLibrary: true } : item,
      );
      const detailLines = [entry.path, ...outcome.linked];
      configStore.show({
        title: t("skills_installed_toast", { name: entry.name || entry.id }),
        detail: detailLines.join("\n"),
        isError: false,
        revealedPath: entry.path,
      });
      return entry;
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
      return null;
    } finally {
      this.saving = false;
    }
  }

  async updateSelectedRemote(): Promise<void> {
    const sel = this.selectedSkill;
    if (!sel?.source) return;
    this.updatingRemote = true;
    try {
      const outcome = await invoke<SkillUpdateOutcome>("skills_update_remote", { id: sel.id });
      this.upsertSkill(outcome.entry);
      if (outcome.updated) {
        const ver = outcome.currentCommit ? `@${outcome.currentCommit}` : "";
        configStore.show({
          title: t("skills_updated_toast", { name: outcome.entry.name || sel.id, version: ver }),
          detail: outcome.backupPath
            ? `${outcome.entry.path}\n${t("cfg_toast_backups", { n: 1 })}`
            : outcome.entry.path,
          isError: false,
          revealedPath: outcome.entry.path,
        });
      } else {
        configStore.show({
          title: t("skills_up_to_date"),
          detail: outcome.currentCommit ? `@${outcome.currentCommit}` : "",
          isError: false,
          revealedPath: "",
        });
      }
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: invokeError(err),
        isError: true,
        revealedPath: "",
      });
    } finally {
      this.updatingRemote = false;
    }
  }
}

export const skillsStore = new SkillsStore();
