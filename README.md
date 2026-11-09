# laneway

Self-hostable kanban board with a web UI, a REST API, an MCP server for AI agents, and git-backed
board history.

laneway is in early development. Nothing is usable yet, see [TODO.md](TODO.md) for where it's at.

## Planned

- Full kanban: columns, swimlanes, checklists, labels, due dates, comments, attachments, views
- Users, workspaces and fine-grained, explainable permissions
- Every change tracked, attributed and revertable, stored as readable text in git per board
- MCP server so agents (Claude, Codex, ...) work on boards next to humans, as their user's agent
- Runs in Docker, every setting configurable via env vars or the UI
- English, German and French from the start

## Self-hosting

Not ready yet. Once released, images are published to `ghcr.io/dablincx/laneway`. Two example
setups to start from:

- [examples/compose.yml](examples/compose.yml): separate app and worker containers
- [examples/compose.aio.yml](examples/compose.aio.yml): everything in one container, for small installs
 All env vars are documented in
[docs/configuration/environment-variables.md](docs/configuration/environment-variables.md).

## Development

Only Docker + Docker Compose are needed on the host:

```bash
./dev.sh up
./dev.sh test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/](docs/README.md).

## License

[GPL-3.0-or-later](LICENSE)
