-- Card details for v0.2: priority, start and due dates, estimate (story points or hours).
ALTER TABLE cards
    ADD COLUMN priority text CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    ADD COLUMN start_at timestamptz,
    ADD COLUMN due_at timestamptz,
    ADD COLUMN estimate double precision CHECK (estimate >= 0 AND estimate <= 100000),
    ADD CONSTRAINT cards_start_before_due CHECK (start_at IS NULL OR due_at IS NULL OR start_at <= due_at);

-- "Due soon" lists across boards (my tasks, reminders).
CREATE INDEX cards_due_idx ON cards (due_at) WHERE due_at IS NOT NULL AND completed_at IS NULL;
