# TODO

Working checklist. `- [x]` done, `- [ ]` not done, *(in progress)* = being worked on.
Section numbers (§) refer to the design plan (`docs/PLAN.md`, kept locally).

## v0.1.0 Foundation (`feat/foundation`)

- [x] Cargo workspace + placeholder crates, web UI skeleton (Vite, React, TanStack Router/Query, Tailwind, shadcn)
- [x] Docker: production image, dev container, compose dev/test files, `dev.sh` (incl. `bump`, native mode), commit-msg hook
- [x] CI (`ci.yml`, `commits.yml`) and release pipeline (`release.yml`)
- [x] `TODO.md` seeded from the roadmap
- [x] Config crate: settings declared once, env parsing, `_FILE` secrets, DB layer + locked fields, generated env docs + `.env.example`
- [x] Logging: text on stdout + daily rotated `.log` / `.json` files (`LANEWAY_LOG_RETENTION_DAYS`)
- [x] Web UI i18n: one Fluent file per language, English fallback, en/de/fr
- [ ] Verify the release workflow end to end with the first `bump: v0.1.0`
- [ ] Verify the dev container (`./dev.sh up` without native mode) on a normal machine; it can't build in cloud sessions
- [x] i18n crate (`fluent-rs`) + `laneway i18n check` (missing/unused keys, syntax) in CI, lint rule against literal UI strings
- [ ] Server: load `/app/locales` with `laneway-i18n` once server-side text exists (emails, notifications), negotiate from the user setting / `Accept-Language`
- [ ] i18n check: also flag literal UI text in JSX attributes (`placeholder`, `title`, `aria-label`, `alt`); today only text nodes are checked
- [x] Postgres 18 + sqlx migrations (advisory lock on start), version check, UUIDv7 everywhere, `/readyz` checks DB
- [x] Job queue crate (serial keys, replace keys for debouncing, delayed jobs, retries with backoff, leases, LISTEN/NOTIFY), worker role runs it
- [ ] Job queue: test the concurrent-claim race (two workers, same serial key) under real parallel load
- [ ] Job queue: show jobs in the admin job monitor (v0.3)
- [x] Config: DB settings layer wired to the `settings` table (encrypted secrets), env-only bootstrap settings, `laneway settings list/set/unset`
- [ ] Config: let required non-bootstrap settings (`LANEWAY_PUBLIC_URL`) come from the DB only; today the first env-only load still needs them in the env
- [ ] Config: rotate `LANEWAY_SECRET_KEY` (re-encrypt stored secrets with the new key)
- [ ] Config: settings UI + API on `laneway-core::settings` (only non-env-only settings, locked fields, `updated_by` set to the admin), apply changes without a restart where possible
- [x] First-launch admin setup with a one-time setup token from the logs, password login (email or username), sessions (cookie, CSRF origin check, login rate limit)
- [x] `LANEWAY_INITIAL_ADMIN_EMAIL` / `_PASSWORD` to skip the setup page
- [ ] Periodic cleanup job for expired sessions (`laneway_db::sessions::delete_expired`) once the worker can schedule recurring jobs
- [ ] Login rate limit is per process and in memory; move it to the DB if several app containers run
- [ ] API error messages are English; localize them with `laneway-i18n` (the web UI already maps error codes to translated text)
- [x] OpenAPI spec (utoipa) at `/api/v1/openapi.json`, docs page at `/api/docs`, TS client generated from it (openapi-typescript + openapi-fetch), CI checks both are up to date
- [ ] Minimal boards -> columns -> cards via `/api/v1`, OpenAPI spec (utoipa), web UI using the generated TS client
- [ ] Action recording for every mutation
- [x] Example compose files (`compose.yml` app + worker, `compose.aio.yml` all in one), tested in CI against the image built there
- [x] CI: dev container build
- [x] CI: Playwright e2e (`web/e2e`, `./dev.sh e2e`) against the Docker image with a real Postgres: setup, login, logout, German UI
- [x] CI: markdown link check (lychee, relative links and fragments in README, CONTRIBUTING, SECURITY, docs, examples)
- [ ] Link check: external links too, in a scheduled job that opens an issue instead of failing pushes
- [ ] CI: coverage gates (`cargo-llvm-cov`)
- [ ] e2e: run the tests in Firefox and WebKit too, not only Chromium
- [ ] e2e: `./dev.sh e2e` without native mode needs Playwright's Chromium in the dev container

## v0.2.0 Full kanban (`feat/kanban`)

- [ ] Columns, swimlanes, drag and drop, WIP limits, done column, archive, board + card templates, duplicate board (§5)
- [ ] Cards: markdown, checklists, subtasks, labels, priority, dates, estimates, custom fields, assignees, watchers, comments, @mentions, attachments + covers, relations, recurring cards, card aging
- [ ] Views: board, list, table, my tasks; filter bar, search, command palette, keyboard shortcuts, dark mode toggle
- [ ] Realtime updates over websockets + presence
- [ ] Trash bin (`LANEWAY_TRASH_RETENTION_DAYS`)
- [ ] Bulk edit (one revertable action)

## v0.3.0 Users, auth & permissions (`feat/auth`, `feat/permissions`)

- [ ] Registration modes, SMTP email (verification, reset, invites, login alerts)
- [ ] TOTP 2FA, sessions list, admin panel, audit log, setup guide (§7, §14)
- [ ] Workspaces, groups, roles + flags, board levels, resolver, sync toggle, `effective_access`, explain endpoints + UI (§8)
- [ ] Workspace + board invites, "Shared with me"

## v0.4.0 History & local git boards (`feat/history`, `feat/git-store`)

- [ ] History UI, diff view, revert, time travel, undo (§6.1)
- [ ] Per-board bare git repos, deterministic text format, one commit per action, `access.yaml` (§6.2)
- [ ] Attachments in Git LFS
- [ ] Workspace history

## v0.5.0 API & MCP (`feat/api`, `feat/mcp`)

- [ ] API pass: docs page, API keys, rate limits, idempotency keys, ETags, cursor pagination, SSE, `/api/v1/instance` (§9)
- [ ] MCP server, agents, agent keys, OAuth 2.1 (PKCE, DCR, consent with agent picker) (§10)
- [ ] MCP tools, resources, prompts; per-board agent access toggle
- [ ] Card claims + handoff notes

## v0.6.0 Release candidate (`feat/release-candidate`)

- [ ] en/de/fr complete, accessibility pass (WCAG 2.2 AA), responsive layout
- [ ] Docs complete (README, self-hosting, env vars, API, MCP, translating, development)
- [ ] Upgrade test from every 0.x
- [ ] Security review, performance check with big boards
- [ ] Extended bug hunt

## v1.0.0

- [ ] Bump from the last 0.6.x once the bug hunt comes up clean

## After 1.0 (§23)

- [ ] Set A: notifications & reminders (prefs, digest, ntfy, Discord/Matrix webhook destinations, per-card reminders, email-to-card)
- [ ] Set B: automations (flow engine, rquickjs scripts, notify + webhook steps, MCP feedback)
- [ ] Set C: remote git sync (remotes, debounced push, GitHub App, three-way merge, mirroring, encrypted fields)
- [ ] Set D: views & productivity (calendar, timeline, focus mode, quick add, flow metrics, iCal, PWA)
- [ ] Set E: sharing & portability (public links, imports, native export)
- [ ] Set F: auth extras (passkeys, OIDC, app login, delta sync)
- [ ] Set G: agent extras (review lane, digest, rate limits, kill switch, approval queue, automation tools)
- [ ] Release images for `linux/arm/v7` (best effort) and riscv64/ppc64le/s390x (optional), plus `edge.yml`, `nightly.yml`, `security.yml`, `cleanup.yml` and the other §18.1 workflows

## Future (§24)

- [ ] Discord app + bot (§13)
- [ ] n8n integration both ways, generic HMAC webhooks, incoming webhook trigger
- [ ] Mobile app (separate repo, §16)
- [ ] Community board templates
- [ ] Community automation nodes
- [ ] Gotify / Slack channels, `laneway` CLI
- [ ] Ideas backlog from §24
