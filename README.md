# laneway

> [!WARNING]
> **laneway is not stable yet.** Every release before v1.0.0 can contain breaking changes: to the
> API, the database, the config and the docker setup. Don't rely on it for anything important yet,
> keep backups, and read the [changelog](CHANGELOG.md) before you update. v1.0.0 comes once the
> roadmap up to it in [TODO.md](TODO.md) is done; there is no date for it yet.

Self-hostable kanban board with a web UI, a REST API, an MCP server for AI agents, and git-backed
board history.

laneway is in early development. v0.1.0 has accounts, boards with columns and cards, the REST
API and English, German and French; much more is coming (see [TODO.md](TODO.md)).

## What works today

- First-launch setup, login with email or username, sessions
- Boards with columns and cards: add, rename, reorder, move, edit, delete
- Every change recorded with an undo step (history and revert come in v0.4)
- REST API at `/api/v1`, OpenAPI spec at `/api/v1/openapi.json`, docs page at `/api/docs`
- English, German and French

## Planned

- Full kanban: swimlanes, checklists, labels, due dates, comments, attachments, more views
- Users, workspaces and fine-grained, explainable permissions
- History and revert, every board stored as readable text in git
- MCP server so agents (Claude, Codex, ...) work on boards next to humans, as their user's agent
- Notifications, automations, sync to your own git remote

## Self-hosting

Images are published to `ghcr.io/dablincx/laneway` for amd64 and arm64. Two example setups to start
from:

- [examples/compose.yml](examples/compose.yml): separate app and worker containers
- [examples/compose.aio.yml](examples/compose.aio.yml): everything in one container, for small installs

Each file explains the steps at the top. All env vars are documented in
[docs/configuration/environment-variables.md](docs/configuration/environment-variables.md). On the
first start, laneway prints a one-time setup token to its logs: open the instance, enter the token
and create the admin account (or set `LANEWAY_INITIAL_ADMIN_EMAIL` and `_PASSWORD` to skip that).

## Development

Only Docker + Docker Compose are needed on the host:

```bash
./dev.sh up
./dev.sh test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/](docs/README.md).

## License

[GPL-3.0-or-later](LICENSE)
