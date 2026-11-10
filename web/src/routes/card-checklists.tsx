import { ListChecks, Trash2 } from "lucide-react";
import { useState } from "react";

import { type Checklist, type ChecklistItem, checklistApi } from "@/api/checklists";
import { errorMessage } from "@/api/errors";
import { FormError } from "@/components/form";
import { InlineEdit } from "@/components/inline-edit";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import { useBoardMutation } from "@/lib/use-board-mutation";

/** Done and total items, for a progress bar or a badge. */
export function progress(items: { done: boolean }[]) {
  return { done: items.filter((i) => i.done).length, total: items.length };
}

/** The checklists of one card, editable. Every change saves right away. */
export function CardChecklists({
  boardId,
  cardId,
  checklists,
  items,
}: {
  boardId: string;
  cardId: string;
  checklists: Checklist[];
  items: ChecklistItem[];
}) {
  const { t } = useI18n();
  const create = useBoardMutation(boardId, (title: string) => checklistApi.create(cardId, title));
  return (
    <div className="flex flex-col gap-4">
      {checklists.map((checklist) => (
        <ChecklistView
          key={checklist.id}
          boardId={boardId}
          checklist={checklist}
          items={items.filter((i) => i.checklist_id === checklist.id)}
        />
      ))}
      <div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => create.mutate([t("checklist-default-title")])}
        >
          <ListChecks className="size-4" aria-hidden />
          {t("checklist-add")}
        </Button>
        <FormError message={create.isError ? errorMessage(t, create.error) : null} />
      </div>
    </div>
  );
}

function ChecklistView({
  boardId,
  checklist,
  items,
}: {
  boardId: string;
  checklist: Checklist;
  items: ChecklistItem[];
}) {
  const { t } = useI18n();
  const rename = useBoardMutation(boardId, (title: string) => checklistApi.rename(checklist.id, title));
  const remove = useBoardMutation(boardId, () => checklistApi.delete(checklist.id));
  const add = useBoardMutation(boardId, (text: string) => checklistApi.addItem(checklist.id, text));
  const update = useBoardMutation(boardId, (id: string, changes: { text?: string; done?: boolean }) =>
    checklistApi.updateItem(id, changes),
  );
  const removeItem = useBoardMutation(boardId, (id: string) => checklistApi.deleteItem(id));
  // Ticks show right away; the board refresh after the save confirms them.
  const [ticked, setTicked] = useState<Record<string, boolean>>({});
  const isDone = (item: ChecklistItem) => ticked[item.id] ?? item.done;
  const { done, total } = progress(items.map((i) => ({ done: isDone(i) })));
  const error = [rename, remove, add, update, removeItem].find((m) => m.isError)?.error;

  return (
    <section aria-label={checklist.title} className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <ListChecks className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        <InlineEdit
          label={t("checklist-rename")}
          value={checklist.title}
          onSave={(title) => rename.mutate([title])}
          className="flex-1 text-sm font-medium"
        />
        <span className="text-xs text-muted-foreground">{t("checklist-progress", { done, total })}</span>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-7 text-muted-foreground"
          aria-label={t("checklist-delete", { title: checklist.title })}
          onClick={() => remove.mutate([])}
        >
          <Trash2 className="size-4" />
        </Button>
      </div>
      {total > 0 && (
        <div className="h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden>
          <div className="h-full bg-primary transition-all" style={{ width: `${(done / total) * 100}%` }} />
        </div>
      )}
      <ul className="flex flex-col gap-1">
        {items.map((item) => (
          <li key={item.id} className="group flex items-center gap-2">
            <input
              type="checkbox"
              className="size-4 shrink-0"
              aria-label={item.text}
              checked={isDone(item)}
              onChange={(e) => {
                const next = e.target.checked;
                setTicked((current) => ({ ...current, [item.id]: next }));
                update.mutate([item.id, { done: next }]);
              }}
            />
            <InlineEdit
              label={t("checklist-item-edit")}
              value={item.text}
              onSave={(text) => update.mutate([item.id, { text }])}
              className={`flex-1 text-sm ${isDone(item) ? "text-muted-foreground line-through" : ""}`}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-6 text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              aria-label={t("checklist-item-delete", { text: item.text })}
              onClick={() => removeItem.mutate([item.id])}
            >
              <Trash2 className="size-3.5" />
            </Button>
          </li>
        ))}
      </ul>
      {/* Not a <form>: this sits inside the card dialog's form. Enter adds the item and keeps
          the focus here, so several items can be typed in a row. */}
      <Input
        aria-label={t("checklist-item-new")}
        placeholder={t("checklist-item-new")}
        className="h-8"
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          e.preventDefault();
          const input = e.currentTarget;
          const text = input.value.trim();
          if (text) add.mutate([text], { onSuccess: () => (input.value = "") });
        }}
      />
      <FormError message={error ? errorMessage(t, error) : null} />
    </section>
  );
}
