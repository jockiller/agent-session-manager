#!/usr/bin/env bash
# Release notes from conventional commits between the previous tag and this one.
# Usage: release-notes.sh [tag-or-revision]
# With no argument, uses the tag being built in GitHub Actions, otherwise HEAD.
set -euo pipefail

target="${1:-}"
if [ -z "$target" ]; then
  if [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
    target="$GITHUB_REF_NAME"
  else
    target="HEAD"
  fi
fi

if ! git rev-parse --verify --quiet "$target" >/dev/null; then
  echo "unknown revision: $target" >&2
  exit 1
fi

version=$(git describe --tags --exact-match "$target" 2>/dev/null || echo "$target")
prev=$(git describe --tags --abbrev=0 "${target}^" 2>/dev/null || true)
if [ -n "$prev" ]; then
  range="${prev}..${target}"
else
  range="$target"
fi

repo="${GITHUB_REPOSITORY:-}"
if [ -z "$repo" ]; then
  url=$(git remote get-url origin 2>/dev/null || true)
  repo=$(printf '%s\n' "$url" | sed -E 's#(git@[^:]+:|https://[^/]+/|ssh://git@[^/]+/)##; s#\.git$##')
fi

echo "## Agent Session Manager ${version}"
echo
echo "Cross-platform desktop session manager for AI Agent CLIs & developer tools (Claude Code, OpenAI Codex, OpenCode, OpenClaw, Gemini, Qwen, DeepSeek, Cursor, Hermes, etc.)."
echo
echo "### Changes"
echo

emit_section() {
  title="$1"
  pattern="$2"
  matches=$(git log --no-merges --pretty=format:'%s' "$range" | grep -E "$pattern" || true)
  if [ -n "$matches" ]; then
    echo "#### ${title}"
    echo
    printf '%s\n' "$matches" | sed 's/^/- /'
    echo
  fi
}

emit_section "Features" '^feat(\([^)]*\))?(!)?:'
emit_section "Fixes" '^fix(\([^)]*\))?(!)?:'
emit_section "Maintenance" '^(refactor|perf|docs|style|chore|test|build|ci)(\([^)]*\))?(!)?:'

unmatched=$(git log --no-merges --pretty=format:'%s' "$range" | grep -Ev '^(feat|fix|refactor|perf|docs|style|chore|test|build|ci)(\([^)]*\))?(!)?:' || true)
if [ -n "$unmatched" ]; then
  echo "#### Other"
  echo
  printf '%s\n' "$unmatched" | sed 's/^/- /'
  echo
fi

if [ -n "$prev" ] && [ -n "$repo" ]; then
  echo "Full changelog: https://github.com/${repo}/compare/${prev}...${version}"
  echo
fi

cat <<'EOF'
### Assets
- **macOS**: `.dmg` (Apple Silicon & Intel)
- **Windows**: `.exe` (NSIS Installer)
- **Linux**: `.AppImage` / `.deb`

### macOS Gatekeeper Note
If you encounter "Application is damaged" or Gatekeeper blocks on macOS, run:

```bash
sudo xattr -dr com.apple.quarantine "/Applications/Agent Session Manager.app"
```
EOF
