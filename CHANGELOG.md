# Changelog

All notable changes to Agent Session Manager are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Changelog entries start with 0.1.7. Earlier releases are documented in
[GitHub Releases](https://github.com/jockiller/agent-session-manager/releases).

## [Unreleased]

## [0.2.2] - 2026-10-31

### Added

- **Per-model pricing**: providers now keep input, output, cache-read and cache-write rates
  (USD per million tokens). Rates are filled from models.dev when pulling a catalog and are
  exported to OpenCode, Pi and omp; the remaining agents are unchanged.

## [0.2.1] - 2026-10-27

### Added

- **Per-model thinking levels**: OpenCode can set a thinking level on each model. Other
  agents keep a single default and ignore the per-model map.

## [0.2.0] - 2026-10-27

### Added

- **Agent config manager**: a new "Config" mode in the title bar manages model providers,
  MCP servers and AGENTS.md in one place and exports them to OpenCode, DSH, ZCode, Grok, Pi
  and omp. Providers can pull their model list from the relay's catalog, enriched with
  context, output, input modalities and thinking levels from models.dev. MCP servers can be
  imported by pasting Claude / OpenCode / Codex JSON or Grok TOML. Each agent picks its own
  models, default model, thinking level and MCP servers.
- **Safe export**: every file an export changes is first backed up under the app data
  directory (latest 20 exports are kept); unchanged files are not rewritten, and a second
  export with the same settings is a no-op. Entries the app does not own are preserved.
- **Google protocol providers**: exported natively to OpenCode, Pi and omp. ZCode, DSH and
  Grok cannot speak it, so those providers are skipped there unless a verified
  OpenAI-compatible Chat URL is set ("Check Chat compatibility").

### Fixed

- **Anthropic thinking on DSH and omp**: newer Claude models on DSH are marked for adaptive
  thinking, and omp writes the thinking mode each Claude generation expects.
- **Antigravity sessions no longer stay "running"**: Antigravity leaves
  `presence/<id>.lock` behind after a conversation ends, so running state is now probed
  through the file lock instead of the file's existence (the same check DSH uses).
- Modal backdrops no longer trigger accessibility warnings.

### Removed

- **Token analytics**: token, cache-hit and throughput statistics are gone from the session
  list, inspector, cleanup dialog and title bar. Session size, turns and timing remain.

## [0.1.8] - 2026-10-25

### Fixed

- **OpenCode archived sessions**: OpenCode V2 stores archive state as
  `session_v2.time_archived`. The adapter now treats a non-null timestamp as
  archived, so those sessions appear in the Archived sidebar view. Databases
  without the column keep scanning as unarchived.

## [0.1.7] - 2026-10-19

### Added

- **Archived sessions view**: a new "Archived" quick view in the sidebar lists the sessions
  archived in the agent's own UI, with a count badge next to it and a violet `Archived` marker
  on each session card. The label is localized in all five interface languages.
- **DSH token analytics**: prompt, completion and cache-read/write totals are read from DSH's
  projection cache, so DSH sessions now report token usage and feed the aggregate counters.

### Fixed

- **DSH sessions no longer all show as running**: `session.lock` is a file DSH never deletes;
  it only excludes other writers with a non-blocking `flock(2)` while a session is open. Running
  state is now probed through that lock instead of through the file's existence.
- **DSH titles, logs and timestamps**: DSH writes `session.v4.jsonl.zstd`, while the adapter only
  looked for v3/v2 names. It now discovers the newest `session[.vN].jsonl[.zstd]` generation,
  which restores titles, transcript availability, working directory, creation/update times and
  list ordering.
- **DSH conversation content**: v4 events are decoded (`assistant/message` content blocks,
  `data.content` user text, `turn/start` for turn counts, tool calls), and injected context such
  as runtime context, skill catalogs, approval notices and job notifications is no longer shown
  as user messages or used as the first-prompt snippet.
- **DSH cleanup**: deleting a session now removes its record from the current per-session
  projection-cache layout (`storages/session_projcache/sessions/<id>.json`); the old single-file
  cache path no longer exists.
- **Untitled DSH sessions** are left without a title so the interface can localize the
  "Untitled Session" label instead of showing a hardcoded Chinese placeholder.
