# Settings in the database

Most settings can be stored in the database instead of the env. The settings UI will use this
later; until then admins use the `laneway settings` commands. Keys are the env var names from
the [environment variables](environment-variables.md) reference.

Precedence: env var > database > default. A setting that is set in the env is *locked*: the
stored value is kept but not used until the env var is removed.

Settings marked *env var only* (secret key, database URL and pool size, process role, HTTP
port, data directory, log level and retention) are needed before the database is reachable or
only apply at startup. They can't be stored, and values found in the database for them are
ignored.

## Commands

The commands read the same env as `laneway serve` (at least `LANEWAY_DATABASE_URL` and
`LANEWAY_SECRET_KEY`), so run them in the laneway container, e.g.
`docker compose exec app laneway settings list`.

| Command | What it does |
|---|---|
| `laneway settings list` | Every setting with its effective value (secrets masked) and where it comes from: `env`, `env file`, `database` or `default`, plus `locked by env` when the env overrides a stored value. |
| `laneway settings set <NAME> [VALUE]` | Validates and stores a value. Without `VALUE` it reads one line from stdin, so secrets stay out of the shell history. |
| `laneway settings unset <NAME>` | Removes a stored value, so the env var or the default applies again. |

Changes take effect when laneway restarts.

## How values are stored

- Values are checked with the same parser as at startup before they are saved, so a bad value
  is refused instead of breaking the next start.
- Secret settings are encrypted with AES-256-GCM. The key is derived from `LANEWAY_SECRET_KEY`
  (HKDF-SHA256, info `laneway settings v1`), and the setting name is bound to the ciphertext.
  The stored text looks like `enc:v1:<nonce>:<ciphertext>`.
- If a stored value can't be used at startup (it no longer parses, the setting was removed, or
  it can't be decrypted because `LANEWAY_SECRET_KEY` changed), laneway logs a warning naming
  the key and falls back to the env or the default. `laneway settings list` shows the same
  warnings. Set the value again or unset it to clear the warning.
