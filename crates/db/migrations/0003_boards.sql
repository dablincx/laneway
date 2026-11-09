-- Workspaces. Every user gets a personal one; shared workspaces come with permissions (v0.3).
CREATE TABLE workspaces (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    name            text NOT NULL,
    owner_id        uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    is_personal     boolean NOT NULL DEFAULT false,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX workspaces_personal_uidx ON workspaces (owner_id) WHERE is_personal;

INSERT INTO workspaces (name, owner_id, is_personal)
SELECT name, id, true FROM users;

CREATE TABLE boards (
    id                      uuid PRIMARY KEY DEFAULT uuidv7(),
    workspace_id            uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    owner_id                uuid NOT NULL REFERENCES users (id),
    name                    text NOT NULL,
    description             text NOT NULL DEFAULT '',
    agent_access_enabled    boolean NOT NULL DEFAULT true,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX boards_workspace_idx ON boards (workspace_id);
CREATE INDEX boards_owner_idx ON boards (owner_id);

-- Positions are fractional index keys compared byte by byte (COLLATE "C"), so moving
-- something only changes its own row.
CREATE TABLE columns (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    board_id    uuid NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    name        text NOT NULL,
    position    text COLLATE "C" NOT NULL,
    wip_limit   integer CHECK (wip_limit > 0),
    is_done     boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX columns_board_idx ON columns (board_id, position);

CREATE TABLE cards (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    board_id        uuid NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    column_id       uuid NOT NULL REFERENCES columns (id),
    position        text COLLATE "C" NOT NULL,
    title           text NOT NULL,
    description     text NOT NULL DEFAULT '',
    created_by      uuid NOT NULL REFERENCES users (id),
    completed_at    timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX cards_column_idx ON cards (column_id, position);
CREATE INDEX cards_board_idx ON cards (board_id);

-- Every change to a board is an action: who did what, and how to undo it.
-- History UI, revert and git commits build on this (v0.4).
CREATE TABLE actions (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    board_id            uuid NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    actor_type          text NOT NULL CHECK (actor_type IN ('user', 'agent', 'api_key', 'system', 'sync')),
    actor_id            uuid,
    on_behalf_of        uuid,
    type                text NOT NULL,
    payload             jsonb NOT NULL,
    inverse             jsonb NOT NULL,
    git_commit_sha      text,
    reverted_by         uuid REFERENCES actions (id),
    created_at          timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX actions_board_idx ON actions (board_id, id);
