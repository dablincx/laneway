-- Board labels. Colors come from a fixed palette so they work in light and dark mode.
CREATE TABLE labels (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    board_id    uuid NOT NULL REFERENCES boards (id) ON DELETE CASCADE,
    name        text NOT NULL,
    color       text NOT NULL CHECK (color IN ('gray', 'red', 'orange', 'yellow', 'green', 'teal', 'blue', 'purple', 'pink')),
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX labels_board_name_uidx ON labels (board_id, lower(name));

CREATE TABLE card_labels (
    card_id     uuid NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    label_id    uuid NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (card_id, label_id)
);
CREATE INDEX card_labels_label_idx ON card_labels (label_id);
