# CLAUDE.md

Rules for Claude working on laneway. Read this first in every session.

## Start of a session

- The design lives in `docs/PLAN.md`, which is **gitignored** (local only). If it's missing, ask the
  maintainer to upload the plan file before doing anything that depends on it, then save it as
  `docs/PLAN.md`. `TODO.md` is tracked and says what's done and what's next.
- When unsure about anything, ask the maintainer instead of guessing. Questions are always fine.

## Hard rules

- **The repo is public.** Never commit credentials, tokens, private hostnames, emails, personal info
  or the plan file. Dev-only dummy values (like the dev `LANEWAY_SECRET_KEY`) must say so in the value.
- The name is **laneway**, always lowercase, also at the start of a sentence, in UI text, docs and
  commits. Only env vars are uppercase (`LANEWAY_*`).
- No emojis anywhere: code, docs, UI strings, commit messages, PRs.
- No hardcoded UI text. Every string goes into `locales/en.ftl` (and `de.ftl`, `fr.ftl` where
  translated). Missing translations fall back to English.
- Never set up or touch a production install. The instance URL is always config.

## Workflow

- Branches: `feat/<group>` per feature group, `feat/<group>-<feature>` for the work itself (git
  can't have `feat/x` and `feat/x/y` at once). Also `fix/<topic>`, `docs/<topic>`, `chore/<topic>`,
  `ci/<topic>`, `refactor/<topic>`. Never work on `claude/*` session branches.
- Sub-branch -> feature branch when it works, feature branch -> `main` via PR when the group is done
  and CI is green. Merge commits, no squash (the changelog needs every commit).
- Commits: Conventional Commits, `type(scope): description`, lowercase. Types: feat, fix, perf,
  refactor, docs, test, style, build, ci, chore, revert, bump, changelog. Checked by
  `scripts/git-hooks/commit-msg` and CI.
- Update `TODO.md` in the same commit that finishes or adds something. New gaps, shortcuts and ideas
  go in as unchecked items right away.
- Every feature ships with tests, every bug fix with a regression test that fails without the fix.
- Versions: `./dev.sh bump <major|minor|patch>`, only on `main`, never on a feature branch. The
  `bump:` commit on main triggers the release workflow (changelog, tag, `release/vX.Y.Z`, images on
  ghcr.io, GitHub Release).
- After each finished feature: bug hunt (clippy, tests, reread the diff, edge cases, permissions,
  error handling), fix each find on a `fix/<topic>` branch with a test, bump patch, repeat until
  nothing obvious is left.

## Running things

- `./dev.sh help` lists everything. `./dev.sh lint` and `./dev.sh test` must pass before pushing.
- In Claude Code cloud sessions use `LANEWAY_DEV_NATIVE=1 ./dev.sh ...`: the toolchains are on the
  host, and containers there can't reach the network through the session proxy. Services
  (Postgres 18, Mailpit, ntfy) still run in Docker via `./dev.sh services`.
- After changing settings in `crates/config`, run `./dev.sh gen` to regenerate
  `docs/configuration/environment-variables.md` and `examples/.env.example` (CI checks them).

## Layout

- `crates/server`: the `laneway` binary (axum app, worker role, CLI). `crates/config`: every setting,
  declared once. Other crates are placeholders until their roadmap item starts (see `TODO.md`).
- `web/`: React + TS UI. It only talks to the public `/api/v1`.
- `locales/`: one Fluent file per language, shared by server and web. Ids are prefixed by area
  (`common-`, `board-`, `card-`, ...) and grouped with `## Section` comments.
- `docker/`: production image, dev container, compose files. `examples/`: self-hosting compose.
