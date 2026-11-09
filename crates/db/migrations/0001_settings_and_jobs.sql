-- Instance settings saved from the UI. Keys are env var names (see crates/config).
-- Secret values are stored encrypted (encrypted = true), never in plain text.
CREATE TABLE settings (
    key         text PRIMARY KEY,
    value       text NOT NULL,
    encrypted   boolean NOT NULL DEFAULT false,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    updated_by  uuid
);

-- Background job queue (crates/jobs).
CREATE TABLE jobs (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    kind            text NOT NULL,
    payload         jsonb NOT NULL DEFAULT '{}',
    status          text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'done', 'failed')),
    -- Jobs with the same serial key run one at a time, in enqueue order (e.g. one board's git
    -- commits). A job waiting for a retry blocks the jobs behind it.
    serial_key      text,
    -- Enqueueing with a replace key replaces a still-queued job of the same kind + key
    -- (e.g. a debounced git push whose timer resets on every commit).
    replace_key     text,
    run_at          timestamptz NOT NULL DEFAULT now(),
    attempts        integer NOT NULL DEFAULT 0,
    max_attempts    integer NOT NULL DEFAULT 5,
    last_error      text,
    locked_by       text,
    lease_until     timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    finished_at     timestamptz
);

-- Picking the next job.
CREATE INDEX jobs_queued_idx ON jobs (run_at, id) WHERE status = 'queued';
-- Ordering inside a serial key.
CREATE INDEX jobs_serial_queued_idx ON jobs (serial_key, id) WHERE status = 'queued' AND serial_key IS NOT NULL;
-- At most one running job per serial key. A second worker claiming one fails on this and retries.
CREATE UNIQUE INDEX jobs_serial_running_uidx ON jobs (serial_key) WHERE status = 'running' AND serial_key IS NOT NULL;
-- At most one queued job per kind + replace key.
CREATE UNIQUE INDEX jobs_replace_queued_uidx ON jobs (kind, replace_key) WHERE status = 'queued' AND replace_key IS NOT NULL;
-- Cleanup of finished jobs.
CREATE INDEX jobs_finished_idx ON jobs (finished_at) WHERE status IN ('done', 'failed');
