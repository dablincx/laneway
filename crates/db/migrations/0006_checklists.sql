-- Checklists on cards. A card can have several, each with its own items.
CREATE TABLE checklists (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    card_id     uuid NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    title       text NOT NULL,
    position    text COLLATE "C" NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX checklists_card_idx ON checklists (card_id, position);

CREATE TABLE checklist_items (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    checklist_id    uuid NOT NULL REFERENCES checklists (id) ON DELETE CASCADE,
    text            text NOT NULL,
    done            boolean NOT NULL DEFAULT false,
    position        text COLLATE "C" NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX checklist_items_checklist_idx ON checklist_items (checklist_id, position);
