-- Accounts. Agents and API keys get their own tables later (v0.5).
CREATE TABLE users (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    email           text NOT NULL,
    username        text NOT NULL,
    name            text NOT NULL,
    locale          text,
    timezone        text,
    instance_role   text NOT NULL DEFAULT 'user' CHECK (instance_role IN ('admin', 'user')),
    disabled        boolean NOT NULL DEFAULT false,
    -- argon2id PHC string. NULL = no password login (e.g. SSO-only, later).
    password_hash   text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

-- Case-insensitive uniqueness, so "Alex" and "alex" can't both exist.
CREATE UNIQUE INDEX users_email_uidx ON users (lower(email));
CREATE UNIQUE INDEX users_username_uidx ON users (lower(username));

-- Login sessions. The cookie holds a random token; only its SHA-256 hash is stored.
CREATE TABLE sessions (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id         uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash      bytea NOT NULL UNIQUE,
    user_agent      text,
    ip              inet,
    created_at      timestamptz NOT NULL DEFAULT now(),
    last_seen_at    timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL
);

CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expires_idx ON sessions (expires_at);

-- One-time token for creating the first admin, printed to the logs on start while no admin
-- exists. Only the hash is stored, so any app process can check it.
CREATE TABLE setup_tokens (
    token_hash      bytea PRIMARY KEY,
    created_at      timestamptz NOT NULL DEFAULT now()
);
