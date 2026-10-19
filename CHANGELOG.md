# Changelog

All notable changes to Agent Session Manager are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Changelog entries start with 0.1.7. Earlier releases are documented in
[GitHub Releases](https://github.com/jockiller/agent-session-manager/releases).

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
