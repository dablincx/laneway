import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createRoute, redirect } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight, Trash2 } from "lucide-react";
import { useState } from "react";

import { boardApi, boardQuery, type Card, type Column } from "@/api/boards";
import { errorMessage } from "@/api/errors";
import { AppHeader } from "@/components/app-header";
import { FormError } from "@/components/form";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { useI18n } from "@/i18n";
import { rootRoute } from "./root";
import { meQuery } from "./session";

/** Runs a board change and refreshes the board afterwards. */
function useBoardMutation<A extends unknown[]>(boardId: string, fn: (...args: A) => Promise<unknown>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (args: A) => fn(...args),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["board", boardId] }),
  });
}

function BoardPage() {
  const { boardId } = boardRoute.useParams();
  const { t } = useI18n();
  const board = useQuery(boardQuery(boardId));
  const [openCard, setOpenCard] = useState<string | null>(null);
  const addColumn = useBoardMutation(boardId, (name: string) => boardApi.addColumn(boardId, name));
  const rename = useBoardMutation(boardId, (name: string) => boardApi.rename(boardId, name));

  if (board.isError) {
    return (
      <div className="min-h-screen">
        <AppHeader />
        <p className="p-6 text-sm text-muted-foreground">{t("board-not-found")}</p>
      </div>
    );
  }
  if (!board.data) return null;
  const { columns, cards } = board.data;
  const card = cards.find((c) => c.id === openCard);

  return (
    <div className="flex h-screen flex-col">
      <AppHeader>
        <InlineEdit
          label={t("board-rename")}
          value={board.data.board.name}
          onSave={(name) => rename.mutate([name])}
          className="font-medium"
        />
      </AppHeader>
      <FormError message={rename.isError ? errorMessage(t, rename.error) : null} />
      <main className="flex flex-1 gap-3 overflow-x-auto p-4">
        {columns.map((column, index) => (
          <ColumnView
            key={column.id}
            boardId={boardId}
            column={column}
            index={index}
            columnCount={columns.length}
            cards={cards.filter((c) => c.column_id === column.id)}
            onOpenCard={setOpenCard}
          />
        ))}
        <form
          className="flex w-72 shrink-0 flex-col gap-2 self-start"
          onSubmit={(e) => {
            e.preventDefault();
            const form = e.currentTarget;
            addColumn.mutate([String(new FormData(form).get("name") ?? "")], {
              onSuccess: () => form.reset(),
            });
          }}
        >
          <Input name="name" aria-label={t("column-new-name")} placeholder={t("column-new-name")} required />
          <FormError message={addColumn.isError ? errorMessage(t, addColumn.error) : null} />
        </form>
      </main>
      {card && (
        <CardDialog boardId={boardId} card={card} columns={columns} onClose={() => setOpenCard(null)} />
      )}
    </div>
  );
}

function ColumnView({
  boardId,
  column,
  index,
  columnCount,
  cards,
  onOpenCard,
}: {
  boardId: string;
  column: Column;
  index: number;
  columnCount: number;
  cards: Card[];
  onOpenCard: (id: string) => void;
}) {
  const { t } = useI18n();
  const addCard = useBoardMutation(boardId, (title: string) => boardApi.addCard(column.id, title));
  const rename = useBoardMutation(boardId, (name: string) => boardApi.renameColumn(column.id, name));
  const move = useBoardMutation(boardId, (to: number) => boardApi.moveColumn(column.id, to));
  const remove = useBoardMutation(boardId, () => boardApi.deleteColumn(column.id));
  const error = [addCard, rename, move, remove].find((m) => m.isError)?.error;
  const overLimit = column.wip_limit != null && cards.length > column.wip_limit;

  return (
    <section
      aria-label={column.name}
      className="flex max-h-full w-72 shrink-0 flex-col gap-2 self-start rounded-lg border bg-muted/40 p-2"
    >
      <div className="flex items-center gap-1">
        <InlineEdit
          label={t("column-rename")}
          value={column.name}
          onSave={(name) => rename.mutate([name])}
          className="flex-1 text-sm font-medium"
        />
        <span className={overLimit ? "text-xs text-destructive" : "text-xs text-muted-foreground"}>
          {column.wip_limit != null ? `${cards.length}/${column.wip_limit}` : cards.length}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="size-7 text-muted-foreground"
          aria-label={t("column-move-left")}
          disabled={index === 0}
          onClick={() => move.mutate([index - 1])}
        >
          <ChevronLeft className="size-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-7 text-muted-foreground"
          aria-label={t("column-move-right")}
          disabled={index === columnCount - 1}
          onClick={() => move.mutate([index + 1])}
        >
          <ChevronRight className="size-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="size-7 text-muted-foreground"
          aria-label={t("column-delete")}
          disabled={cards.length > 0}
          onClick={() => remove.mutate([])}
        >
          <Trash2 className="size-4" />
        </Button>
      </div>
      <ul className="flex flex-col gap-2 overflow-y-auto">
        {cards.map((card) => (
          <li key={card.id}>
            <button
              type="button"
              onClick={() => onOpenCard(card.id)}
              className="w-full rounded-md border bg-background px-3 py-2 text-left text-sm shadow-xs hover:bg-accent"
            >
              {card.title}
            </button>
          </li>
        ))}
      </ul>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          const form = e.currentTarget;
          addCard.mutate([String(new FormData(form).get("title") ?? "")], { onSuccess: () => form.reset() });
        }}
      >
        <Input name="title" aria-label={t("card-new-title")} placeholder={t("card-new-title")} required />
      </form>
      <FormError message={error ? errorMessage(t, error) : null} />
    </section>
  );
}

function CardDialog({
  boardId,
  card,
  columns,
  onClose,
}: {
  boardId: string;
  card: Card;
  columns: Column[];
  onClose: () => void;
}) {
  const { t } = useI18n();
  const save = useBoardMutation(boardId, (title: string, description: string) =>
    boardApi.updateCard(card.id, title, description),
  );
  const move = useBoardMutation(boardId, (columnId: string) =>
    boardApi.moveCard(card.id, columnId, 1_000_000),
  );
  const remove = useBoardMutation(boardId, () => boardApi.deleteCard(card.id));
  const error = [save, move, remove].find((m) => m.isError)?.error;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent aria-describedby={undefined}>
        <DialogTitle>{t("card-edit-heading")}</DialogTitle>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            const form = new FormData(e.currentTarget);
            save.mutate([String(form.get("title") ?? ""), String(form.get("description") ?? "")], {
              onSuccess: onClose,
            });
          }}
        >
          <div className="flex flex-col gap-2">
            <Label htmlFor="card-title">{t("card-field-title")}</Label>
            <Input id="card-title" name="title" defaultValue={card.title} required />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="card-description">{t("card-field-description")}</Label>
            <Textarea id="card-description" name="description" defaultValue={card.description} />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="card-column">{t("card-field-column")}</Label>
            <select
              id="card-column"
              className="h-9 rounded-md border bg-transparent px-2 text-sm"
              value={card.column_id}
              onChange={(e) => move.mutate([e.target.value])}
            >
              {columns.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </div>
          <FormError message={error ? errorMessage(t, error) : null} />
          <div className="flex justify-between gap-2">
            <Button
              type="button"
              variant="destructive"
              onClick={() => remove.mutate([], { onSuccess: onClose })}
            >
              {t("card-delete")}
            </Button>
            <div className="flex gap-2">
              <DialogClose asChild>
                <Button type="button" variant="outline">
                  {t("common-cancel")}
                </Button>
              </DialogClose>
              <Button type="submit" disabled={save.isPending}>
                {t("common-save")}
              </Button>
            </div>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/** Text that turns into an input on click and saves on Enter or blur. */
function InlineEdit({
  label,
  value,
  onSave,
  className,
}: {
  label: string;
  value: string;
  onSave: (value: string) => void;
  className?: string;
}) {
  const [editing, setEditing] = useState(false);
  if (!editing) {
    return (
      <button
        type="button"
        aria-label={label}
        className={`truncate text-left ${className ?? ""}`}
        onClick={() => setEditing(true)}
      >
        {value}
      </button>
    );
  }
  const finish = (next: string) => {
    setEditing(false);
    if (next.trim() && next !== value) onSave(next);
  };
  return (
    <Input
      autoFocus
      aria-label={label}
      defaultValue={value}
      className="h-7"
      onBlur={(e) => finish(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(e.currentTarget.value);
        if (e.key === "Escape") setEditing(false);
      }}
    />
  );
}

export const boardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/boards/$boardId",
  beforeLoad: async ({ context }) => {
    if (!(await context.queryClient.ensureQueryData(meQuery))) throw redirect({ to: "/login" });
  },
  component: BoardPage,
});
