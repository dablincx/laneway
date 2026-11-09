# Contributing to laneway

## Setup

The host only needs Docker and Docker Compose. Everything else runs in the dev container.

```bash
./dev.sh up        # Postgres 18, Mailpit, ntfy and the dev container
./dev.sh server    # laneway server on :3000, rebuilds on change
./dev.sh web       # web UI on :5173 (talks to the server through Vite's proxy)
./dev.sh test      # all tests
./dev.sh lint      # everything CI checks
./dev.sh help      # the rest
```

If you have the Rust and Node toolchains installed locally, `LANEWAY_DEV_NATIVE=1 ./dev.sh ...` runs
commands on the host and only the services in Docker. Mailpit's UI is on http://localhost:8025.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`, e.g.
`feat(cards): bulk edit` or `fix(git-store): stable key order`. `./dev.sh` installs a commit-msg hook
that checks this, and CI checks it again.

| Type | For | Changelog |
|---|---|---|
| `feat` | new features | Features |
| `fix` | bug fixes | Bug fixes |
| `perf` | performance | Performance |
| `refactor` | no behaviour change | Refactoring |
| `docs` | documentation | Documentation |
| `build` | build system, dependencies, Docker | Build |
| `revert` | reverting a commit | Reverts |
| `test`, `style`, `ci`, `chore` | tests, formatting, workflows, maintenance | hidden |
| `bump`, `changelog` | release commits only | hidden |

`!` after the type or a `BREAKING CHANGE:` footer marks a breaking change.

## Branches

- `main` is always releasable and only gets merges and release commits.
- `feat/<group>` per feature group, `feat/<group>-<feature>` for the actual work. `fix/<topic>`,
  `docs/<topic>`, `chore/<topic>`, `ci/<topic>`, `refactor/<topic>` for the rest.
- Merges keep the individual commits (no squash), so the changelog sees all of them.

## Tests

Every feature comes with tests, every bug fix with a regression test that fails without the fix.

## Translations

UI text lives in `locales/<language>.ftl` ([Fluent](https://projectfluent.org/)). To add a language,
copy `locales/en.ftl` to `locales/<code>.ftl`, translate it, and add a line to
`locales/languages.toml`. Anything not translated yet falls back to English.

## Releases

Maintainers run `./dev.sh bump <major|minor|patch>` on `main` and push. The release workflow writes
the changelog, tags `vX.Y.Z`, keeps a `release/vX.Y.Z` branch, publishes multi-arch images to
`ghcr.io/dablincx/laneway` and creates the GitHub Release.
