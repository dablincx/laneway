#!/usr/bin/env bash
# Writes CHANGELOG.md and release-notes.md for a release. Used by the release workflow.
# Usage: scripts/release-notes.sh <version without v> [release-notes output path]
#
# Internal changes are left out of the changelog (cliff.toml), so a release can have no visible
# entries. git-cliff then leaves that version out entirely; this adds a short section instead, so
# the changelog commit and the release notes are never empty.
set -euo pipefail
cd "$(dirname "$0")/.."
version="${1:?usage: scripts/release-notes.sh <version>}"
notes="${2:-release-notes.md}"
nothing="No changes to the app itself in this release (internal fixes only)."

git cliff --tag "v$version" -o CHANGELOG.md
if ! grep -q "^## ${version//./\\.} " CHANGELOG.md; then
  section="## $version ($(date -u +%Y-%m-%d))"$'\n\n'"$nothing"$'\n'
  # Put it above the newest existing version (or at the end if there is none yet).
  awk -v section="$section" '
    !done && /^## / { print section; done = 1 }
    { print }
    END { if (!done) { print ""; print section } }
  ' CHANGELOG.md > CHANGELOG.md.tmp
  mv CHANGELOG.md.tmp CHANGELOG.md
fi

git cliff --tag "v$version" --unreleased --strip all > "$notes"
# Only the heading means there's nothing to list.
if ! grep -q "^- " "$notes"; then
  printf '%s\n' "$nothing" > "$notes"
fi
