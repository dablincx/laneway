import {
  closestCorners,
  DndContext,
  type DragEndEvent,
  type DragOverEvent,
  DragOverlay,
  type DragStartEvent,
  type KeyboardCoordinateGetter,
  KeyboardSensor,
  MeasuringStrategy,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  horizontalListSortingStrategy,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createRoute, redirect } from "@tanstack/react-router";
import { AlignLeft, CalendarDays, Check, GripVertical, Settings2, Tags } from "lucide-react";
import { createContext, lazy, Suspense, useContext, useRef, useState } from "react";

import {
  type Label as BoardLabel,
  boardApi,
  boardQuery,
  type Card,
  type CardChanges,
  type Column,
  type Priority,
  priorities,
} from "@/api/boards";
import { errorMessage } from "@/api/errors";
import { AppHeader } from "@/components/app-header";
import { FormError } from "@/components/form";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { useI18n } from "@/i18n";
import {
  type Layout,
  layoutOf,
  moveCard as moveCardIn,
  moveColumn as moveColumnIn,
  placeOf,
} from "@/lib/board-layout";
import { formatDay, fromDateInput, isOverdue, toDateInput } from "@/lib/dates";
import { useBoardMutation } from "@/lib/use-board-mutation";
import { LabelChip, LabelPicker, LabelsDialog } from "./board-labels";
import { rootRoute } from "./root";
import { meQuery } from "./session";

function BoardPage() {
  const { boardId } = boardRoute.useParams();
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const board = useQuery(boardQuery(boardId));
  const [openCard, setOpenCard] = useState<string | null>(null);
  const [settingsFor, setSettingsFor] = useState<string | null>(null);
  const [labelsOpen, setLabelsOpen] = useState(false);
  // While dragging (and until the server has the result) the board shows this layout.
  const [dragLayout, setDragLayoutState] = useState<Layout | null>(null);
  // dnd-kit can call a handler from an earlier render (the keyboard sensor does), so the
  // handlers read the layout from here instead of from the render they were made in.
  const dragLayoutRef = useRef<Layout | null>(null);
  const setDragLayout = (next: Layout | null | ((current: Layout | null) => Layout | null)) => {
    dragLayoutRef.current = typeof next === "function" ? next(dragLayoutRef.current) : next;
    setDragLayoutState(dragLayoutRef.current);
  };
  const [activeId, setActiveId] = useState<string | null>(null);
  const addColumn = useBoardMutation(boardId, (name: string) => boardApi.addColumn(boardId, name));
  const rename = useBoardMutation(boardId, (name: string) => boardApi.rename(boardId, name));
  const moveError = useMutation({ mutationFn: async (e: unknown) => Promise.reject(e) });
  const sensors = useSensors(
    // A few pixels of movement before a drag starts, so clicks still open cards.
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: (event, args) => columnKeyboardCoordinates(event, args, dragLayoutRef.current),
      // Enter opens a card, space picks it up.
      keyboardCodes: { start: ["Space"], cancel: ["Escape"], end: ["Space", "Enter"] },
    }),
  );

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
  const labelById = new Map(board.data.labels.map((l) => [l.id, l]));
  const cardLabelIds = new Map<string, string[]>();
  for (const { card_id, label_id } of board.data.card_labels) {
    cardLabelIds.set(card_id, [...(cardLabelIds.get(card_id) ?? []), label_id]);
  }
  const labelsOf = (cardId: string) =>
    (cardLabelIds.get(cardId) ?? []).flatMap((id) => labelById.get(id) ?? []);
  const serverLayout = layoutOf(columns, cards);
  const layout = dragLayout ?? serverLayout;
  const columnById = new Map(columns.map((c) => [c.id, c]));
  const cardById = new Map(cards.map((c) => [c.id, c]));
  const card = openCard ? cardById.get(openCard) : undefined;
  const settingsColumn = settingsFor ? columnById.get(settingsFor) : undefined;

  const nameOf = (id: string | number | undefined) => {
    if (id == null) return "";
    const key = String(id);
    if (key.startsWith(COLUMN)) return columnById.get(key.slice(COLUMN.length))?.name ?? "";
    return cardById.get(key)?.title ?? "";
  };
  /** The column a drop target belongs to: the column itself or the column of a card. */
  const columnOfTarget = (current: Layout, id: string) =>
    id.startsWith(COLUMN) ? id.slice(COLUMN.length) : placeOf(current, id)?.columnId;

  /** Shows `next` until the server has it, then goes back to the server's layout. */
  const commit = async (next: Layout, request: () => Promise<unknown>) => {
    setDragLayout(next);
    try {
      await request();
    } catch (e) {
      moveError.mutate(e);
    }
    await queryClient.invalidateQueries({ queryKey: ["board", boardId] });
    // A new drag may have started meanwhile; its layout isn't ours to clear.
    if (dragLayoutRef.current === next) setDragLayout(null);
  };

  const onDragStart = ({ active }: DragStartEvent) => {
    moveError.reset();
    setActiveId(String(active.id));
    // A copy, so a save still running from the last drag can tell this layout isn't its own.
    setDragLayout({ ...layout });
  };

  // Cards change columns while they are dragged, so the target column makes room.
  const onDragOver = ({ active, over }: DragOverEvent) => {
    const id = String(active.id);
    if (!over || id.startsWith(COLUMN)) return;
    setDragLayout((current) => {
      if (!current) return current;
      const from = placeOf(current, id);
      const toColumn = columnOfTarget(current, String(over.id));
      if (!from || !toColumn || from.columnId === toColumn) return current;
      const overPlace = placeOf(current, String(over.id));
      const index = overPlace ? overPlace.index : (current.cards[toColumn]?.length ?? 0);
      return moveCardIn(current, id, toColumn, index);
    });
  };

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    setActiveId(null);
    const id = String(active.id);
    const current = dragLayoutRef.current ?? layout;
    if (!over) {
      setDragLayout(null);
      return;
    }
    if (id.startsWith(COLUMN)) {
      const columnId = id.slice(COLUMN.length);
      const target = columnOfTarget(current, String(over.id));
      const from = current.columns.indexOf(columnId);
      const to = target ? current.columns.indexOf(target) : -1;
      if (to === -1 || from === to) {
        setDragLayout(null);
        return;
      }
      void commit(moveColumnIn(current, columnId, to), () => boardApi.moveColumn(columnId, to));
      return;
    }
    let next = current;
    const place = placeOf(current, id);
    const overPlace = placeOf(current, String(over.id));
    if (place && overPlace && place.columnId === overPlace.columnId && place.index !== overPlace.index) {
      next = moveCardIn(current, id, place.columnId, overPlace.index);
    }
    const final = placeOf(next, id);
    const before = placeOf(serverLayout, id);
    if (!final || (before && before.columnId === final.columnId && before.index === final.index)) {
      setDragLayout(null);
      return;
    }
    void commit(next, () => boardApi.moveCard(id, final.columnId, final.index));
  };

  const activeCard = activeId ? cardById.get(activeId) : undefined;
  const activeColumn = activeId?.startsWith(COLUMN)
    ? columnById.get(activeId.slice(COLUMN.length))
    : undefined;

  return (
    <div className="flex h-screen flex-col">
      <AppHeader>
        <InlineEdit
          label={t("board-rename")}
          value={board.data.board.name}
          onSave={(name) => rename.mutate([name])}
          className="font-medium"
        />
        <Button variant="ghost" size="sm" onClick={() => setLabelsOpen(true)}>
          <Tags className="size-4" aria-hidden />
          {t("label-manage")}
        </Button>
      </AppHeader>
      <FormError message={rename.isError ? errorMessage(t, rename.error) : null} />
      <FormError message={moveError.isError ? errorMessage(t, moveError.error) : null} />
      <CardLabelsContext.Provider value={labelsOf}>
        <DndContext
          sensors={sensors}
          collisionDetection={closestCorners}
          // Cards jump between columns while dragging, so positions are measured on every move,
          // not just when the drag starts (else the next arrow key works from stale positions).
          measuring={{ droppable: { strategy: MeasuringStrategy.Always } }}
          onDragStart={onDragStart}
          onDragOver={onDragOver}
          onDragEnd={onDragEnd}
          onDragCancel={() => {
            setActiveId(null);
            setDragLayout(null);
          }}
          accessibility={{
            screenReaderInstructions: { draggable: t("dnd-instructions") },
            announcements: {
              onDragStart: ({ active }) => t("dnd-picked-up", { item: nameOf(active.id) }),
              // Right after picking something up it is "over" itself; that isn't worth saying.
              onDragOver: ({ active, over }) =>
                over && over.id !== active.id
                  ? t("dnd-over", { item: nameOf(active.id), target: nameOf(over.id) })
                  : undefined,
              onDragEnd: ({ active, over }) =>
                over
                  ? t("dnd-dropped", { item: nameOf(active.id), target: nameOf(over.id) })
                  : t("dnd-dropped-nowhere", { item: nameOf(active.id) }),
              onDragCancel: ({ active }) => t("dnd-cancelled", { item: nameOf(active.id) }),
            },
          }}
        >
          <main className="flex flex-1 gap-3 overflow-x-auto p-4">
            <SortableContext
              items={layout.columns.map((id) => COLUMN + id)}
              strategy={horizontalListSortingStrategy}
            >
              {layout.columns.map((columnId) => {
                const column = columnById.get(columnId);
                if (!column) return null;
                const columnCards = (layout.cards[columnId] ?? []).flatMap((cid) => cardById.get(cid) ?? []);
                return (
                  <ColumnView
                    key={columnId}
                    boardId={boardId}
                    column={column}
                    cards={columnCards}
                    onOpenCard={setOpenCard}
                    onSettings={() => setSettingsFor(columnId)}
                  />
                );
              })}
            </SortableContext>
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
              <Input
                name="name"
                aria-label={t("column-new-name")}
                placeholder={t("column-new-name")}
                required
              />
              <FormError message={addColumn.isError ? errorMessage(t, addColumn.error) : null} />
            </form>
          </main>
          <DragOverlay>
            {activeCard ? <CardFace card={activeCard} className="shadow-lg" /> : null}
            {activeColumn ? (
              <div className="w-72 rounded-lg border bg-muted px-3 py-2 text-sm font-medium shadow-lg">
                {activeColumn.name}
              </div>
            ) : null}
          </DragOverlay>
        </DndContext>
      </CardLabelsContext.Provider>
      {card && (
        <CardDialog
          boardId={boardId}
          card={card}
          columns={columns}
          labels={board.data.labels}
          cardLabels={cardLabelIds.get(card.id) ?? []}
          onManageLabels={() => {
            setOpenCard(null);
            setLabelsOpen(true);
          }}
          onClose={() => setOpenCard(null)}
        />
      )}
      {labelsOpen && (
        <LabelsDialog boardId={boardId} labels={board.data.labels} onClose={() => setLabelsOpen(false)} />
      )}
      {settingsColumn && (
        <ColumnSettingsDialog
          boardId={boardId}
          column={settingsColumn}
          cardCount={layout.cards[settingsColumn.id]?.length ?? 0}
          onClose={() => setSettingsFor(null)}
        />
      )}
    </div>
  );
}

/** The labels on a card, resolved from the board. */
const CardLabelsContext = createContext<(cardId: string) => BoardLabel[]>(() => []);

/** Sortable ids of columns start with this, card ids are plain UUIDs. */
const COLUMN = "column:";

/**
 * Arrow keys while dragging with the keyboard. dnd-kit's default guesses from positions, which
 * goes wrong on a board (a card that just changed columns is in its own way, and the column
 * itself competes with the card above). Here left and right go to the next column, up and down
 * to the next card in the same column.
 */
function columnKeyboardCoordinates(
  event: KeyboardEvent,
  args: Parameters<KeyboardCoordinateGetter>[1],
  layout: Layout | null,
): ReturnType<KeyboardCoordinateGetter> {
  const { active, over, droppableRects, collisionRect } = args.context;
  const horizontal = event.code === "ArrowRight" ? 1 : event.code === "ArrowLeft" ? -1 : 0;
  const vertical = event.code === "ArrowDown" ? 1 : event.code === "ArrowUp" ? -1 : 0;
  if ((!horizontal && !vertical) || !layout || !active || !collisionRect) {
    return sortableKeyboardCoordinates(event, args);
  }
  const activeId = String(active.id);
  const isColumn = activeId.startsWith(COLUMN);
  // Where the dragged thing is now: the card or column it is over, else itself.
  const at = String(over?.id ?? active.id);
  const columnId = at.startsWith(COLUMN) ? at.slice(COLUMN.length) : placeOf(layout, at)?.columnId;
  if (!columnId) return undefined;

  if (horizontal) {
    const next = layout.columns[layout.columns.indexOf(columnId) + horizontal];
    const rect = next ? droppableRects.get(COLUMN + next) : undefined;
    if (!rect) return undefined;
    event.preventDefault();
    // Cards go just below the column header, so the column itself is what they're over.
    return { x: rect.left + (rect.width - collisionRect.width) / 2, y: isColumn ? rect.top : rect.top + 8 };
  }

  if (isColumn) return undefined;
  const cards = layout.cards[columnId] ?? [];
  let index = at.startsWith(COLUMN) ? (vertical > 0 ? -1 : cards.length) : cards.indexOf(at);
  let next: string | undefined;
  do {
    index += vertical;
    next = cards[index];
  } while (next === activeId);
  const rect = next ? droppableRects.get(next) : undefined;
  if (!rect) return undefined;
  event.preventDefault();
  return { x: rect.left, y: rect.top };
}

function ColumnView({
  boardId,
  column,
  cards,
  onOpenCard,
  onSettings,
}: {
  boardId: string;
  column: Column;
  cards: Card[];
  onOpenCard: (id: string) => void;
  onSettings: () => void;
}) {
  const { t } = useI18n();
  const addCard = useBoardMutation(boardId, (title: string) => boardApi.addCard(column.id, title));
  const rename = useBoardMutation(boardId, (name: string) => boardApi.updateColumn(column.id, { name }));
  const error = [addCard, rename].find((m) => m.isError)?.error;
  const overLimit = column.wip_limit != null && cards.length > column.wip_limit;
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } =
    useSortable({ id: COLUMN + column.id });

  return (
    <section
      ref={setNodeRef}
      style={{ transform: CSS.Translate.toString(transform), transition }}
      aria-label={column.name}
      className={`flex max-h-full w-72 shrink-0 flex-col gap-2 self-start rounded-lg border bg-muted/40 p-2 ${
        isDragging ? "opacity-50" : ""
      } ${overLimit ? "border-destructive" : ""}`}
    >
      <div className="flex items-center gap-1">
        <button
          type="button"
          ref={setActivatorNodeRef}
          aria-label={t("column-drag")}
          className="cursor-grab rounded p-1 text-muted-foreground hover:bg-accent"
          {...attributes}
          {...listeners}
        >
          <GripVertical className="size-4" />
        </button>
        <InlineEdit
          label={t("column-rename")}
          value={column.name}
          onSave={(name) => rename.mutate([name])}
          className="flex-1 text-sm font-medium"
        />
        {column.is_done && (
          <Check className="size-4 text-muted-foreground" aria-label={t("column-field-done")} />
        )}
        <span
          className={overLimit ? "text-xs font-medium text-destructive" : "text-xs text-muted-foreground"}
          title={overLimit ? t("column-over-limit") : undefined}
        >
          {column.wip_limit != null ? `${cards.length}/${column.wip_limit}` : cards.length}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="size-7 text-muted-foreground"
          aria-label={t("column-settings")}
          onClick={onSettings}
        >
          <Settings2 className="size-4" />
        </Button>
      </div>
      <SortableContext items={cards.map((c) => c.id)} strategy={verticalListSortingStrategy}>
        <ul className="flex min-h-2 flex-col gap-2 overflow-y-auto">
          {cards.map((card) => (
            <SortableCard key={card.id} card={card} onOpen={() => onOpenCard(card.id)} />
          ))}
        </ul>
      </SortableContext>
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

function SortableCard({ card, onOpen }: { card: Card; onOpen: () => void }) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: card.id,
  });
  return (
    <li ref={setNodeRef} style={{ transform: CSS.Translate.toString(transform), transition }}>
      <button
        type="button"
        onClick={onOpen}
        className={`w-full text-left ${isDragging ? "opacity-50" : ""}`}
        {...attributes}
        {...listeners}
      >
        <CardFace card={card} />
      </button>
    </li>
  );
}

function CardFace({ card, className }: { card: Card; className?: string }) {
  const { t, locale } = useI18n();
  const labels = useContext(CardLabelsContext)(card.id);
  const overdue = card.due_at != null && card.completed_at == null && isOverdue(card.due_at);
  const hasBadges = card.priority || card.due_at || card.estimate != null || card.description;
  return (
    <div
      className={`flex flex-col gap-1.5 rounded-md border bg-background px-3 py-2 text-sm shadow-xs hover:bg-accent ${
        className ?? ""
      }`}
    >
      {labels.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {labels.map((label) => (
            <LabelChip key={label.id} label={label} />
          ))}
        </div>
      )}
      <div className="flex items-start gap-2">
        <span className={`flex-1 ${card.completed_at ? "text-muted-foreground line-through" : ""}`}>
          {card.title}
        </span>
        {card.completed_at && (
          <Check className="size-4 shrink-0 text-muted-foreground" aria-label={t("card-done")} />
        )}
      </div>
      {hasBadges && (
        <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          {card.priority && <PriorityBadge priority={card.priority} />}
          {card.due_at && (
            <span
              className={`inline-flex items-center gap-1 rounded px-1 ${overdue ? "bg-destructive/10 text-destructive" : ""}`}
            >
              <CalendarDays className="size-3" aria-hidden />
              {t(overdue ? "card-overdue" : "card-due-on", { date: formatDay(card.due_at, locale) })}
            </span>
          )}
          {card.estimate != null && (
            <span
              className="rounded border px-1"
              title={t("card-estimate-badge", { estimate: card.estimate })}
            >
              {card.estimate}
            </span>
          )}
          {card.description && <AlignLeft className="size-3" aria-label={t("card-has-description")} />}
        </div>
      )}
    </div>
  );
}

/** Spelled out so the translation check can see every key in use. */
const priorityLabel: Record<Priority, string> = {
  low: "card-priority-low",
  medium: "card-priority-medium",
  high: "card-priority-high",
  urgent: "card-priority-urgent",
};

const priorityStyle: Record<Priority, string> = {
  low: "border text-muted-foreground",
  medium: "bg-blue-500/15 text-blue-700 dark:text-blue-300",
  high: "bg-orange-500/15 text-orange-700 dark:text-orange-300",
  urgent: "bg-destructive/15 text-destructive",
};

function PriorityBadge({ priority }: { priority: Priority }) {
  const { t } = useI18n();
  return (
    <span className={`rounded px-1.5 font-medium ${priorityStyle[priority]}`}>
      {t(priorityLabel[priority])}
    </span>
  );
}

function ColumnSettingsDialog({
  boardId,
  column,
  cardCount,
  onClose,
}: {
  boardId: string;
  column: Column;
  cardCount: number;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const save = useBoardMutation(boardId, (changes: Parameters<typeof boardApi.updateColumn>[1]) =>
    boardApi.updateColumn(column.id, changes),
  );
  const remove = useBoardMutation(boardId, () => boardApi.deleteColumn(column.id));
  const error = [save, remove].find((m) => m.isError)?.error;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent aria-describedby={undefined}>
        <DialogTitle>{t("column-settings")}</DialogTitle>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            const form = new FormData(e.currentTarget);
            const limit = String(form.get("wip_limit") ?? "").trim();
            save.mutate(
              [
                {
                  name: String(form.get("name") ?? ""),
                  wip_limit: limit === "" ? null : Number(limit),
                  is_done: form.get("is_done") === "on",
                },
              ],
              { onSuccess: onClose },
            );
          }}
        >
          <div className="flex flex-col gap-2">
            <Label htmlFor="column-name">{t("column-field-name")}</Label>
            <Input id="column-name" name="name" defaultValue={column.name} required />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="column-wip-limit">{t("column-field-wip-limit")}</Label>
            <Input
              id="column-wip-limit"
              name="wip_limit"
              type="number"
              min={1}
              step={1}
              defaultValue={column.wip_limit ?? ""}
              aria-describedby="column-wip-limit-hint"
            />
            <p id="column-wip-limit-hint" className="text-xs text-muted-foreground">
              {t("column-field-wip-limit-hint")}
            </p>
          </div>
          <div className="flex items-start gap-2">
            <input
              id="column-done"
              name="is_done"
              type="checkbox"
              defaultChecked={column.is_done}
              className="mt-1 size-4"
              aria-describedby="column-done-hint"
            />
            <div className="flex flex-col gap-1">
              <Label htmlFor="column-done">{t("column-field-done")}</Label>
              <p id="column-done-hint" className="text-xs text-muted-foreground">
                {t("column-field-done-hint")}
              </p>
            </div>
          </div>
          <FormError message={error ? errorMessage(t, error) : null} />
          <div className="flex justify-between gap-2">
            <Button
              type="button"
              variant="destructive"
              disabled={cardCount > 0}
              onClick={() => remove.mutate([], { onSuccess: onClose })}
            >
              {t("column-delete")}
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

function CardDialog({
  boardId,
  card,
  columns,
  labels,
  cardLabels,
  onManageLabels,
  onClose,
}: {
  boardId: string;
  card: Card;
  columns: Column[];
  labels: BoardLabel[];
  cardLabels: string[];
  onManageLabels: () => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [description, setDescription] = useState(card.description);
  // Cards with a description open on the rendered text, empty ones on the editor.
  const [preview, setPreview] = useState(card.description.trim() !== "");
  const save = useBoardMutation(boardId, (changes: CardChanges) => boardApi.updateCard(card.id, changes));
  const move = useBoardMutation(boardId, (columnId: string) =>
    boardApi.moveCard(card.id, columnId, 1_000_000),
  );
  const remove = useBoardMutation(boardId, () => boardApi.deleteCard(card.id));
  const error = [save, move, remove].find((m) => m.isError)?.error;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent aria-describedby={undefined} className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogTitle>{t("card-edit-heading")}</DialogTitle>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            const form = new FormData(e.currentTarget);
            const text = (key: string) => String(form.get(key) ?? "");
            const estimate = text("estimate").trim();
            save.mutate(
              [
                {
                  title: text("title"),
                  description,
                  priority: (text("priority") || null) as Priority | null,
                  start_at: fromDateInput(text("start_at")),
                  due_at: fromDateInput(text("due_at")),
                  estimate: estimate === "" ? null : Number(estimate),
                },
              ],
              { onSuccess: onClose },
            );
          }}
        >
          <div className="flex flex-col gap-2">
            <Label htmlFor="card-title">{t("card-field-title")}</Label>
            <Input id="card-title" name="title" defaultValue={card.title} required />
          </div>
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="card-description">{t("card-field-description")}</Label>
              <div className="flex gap-1">
                <Button
                  type="button"
                  size="sm"
                  variant={preview ? "ghost" : "secondary"}
                  aria-pressed={!preview}
                  onClick={() => setPreview(false)}
                >
                  {t("card-description-write")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={preview ? "secondary" : "ghost"}
                  aria-pressed={preview}
                  onClick={() => setPreview(true)}
                >
                  {t("card-description-preview")}
                </Button>
              </div>
            </div>
            {preview ? (
              <div className="min-h-24 rounded-md border px-3 py-2" data-testid="card-description-preview">
                {description.trim() ? (
                  <Suspense fallback={<p className="text-sm whitespace-pre-wrap">{description}</p>}>
                    <Markdown text={description} />
                  </Suspense>
                ) : (
                  <p className="text-sm text-muted-foreground">{t("card-description-empty")}</p>
                )}
              </div>
            ) : (
              <>
                <Textarea
                  id="card-description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  className="min-h-40 font-mono text-sm"
                  aria-describedby="card-description-help"
                />
                <p id="card-description-help" className="text-xs text-muted-foreground">
                  {t("card-description-help")}
                </p>
              </>
            )}
          </div>
          <LabelPicker
            boardId={boardId}
            cardId={card.id}
            labels={labels}
            selected={cardLabels}
            onManage={onManageLabels}
          />
          <div className="grid gap-4 sm:grid-cols-2">
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
            <div className="flex flex-col gap-2">
              <Label htmlFor="card-priority">{t("card-field-priority")}</Label>
              <select
                id="card-priority"
                name="priority"
                className="h-9 rounded-md border bg-transparent px-2 text-sm"
                defaultValue={card.priority ?? ""}
              >
                <option value="">{t("card-priority-none")}</option>
                {priorities.map((p) => (
                  <option key={p} value={p}>
                    {t(priorityLabel[p])}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="card-start">{t("card-field-start")}</Label>
              <Input id="card-start" name="start_at" type="date" defaultValue={toDateInput(card.start_at)} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="card-due">{t("card-field-due")}</Label>
              <Input id="card-due" name="due_at" type="date" defaultValue={toDateInput(card.due_at)} />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="card-estimate">{t("card-field-estimate")}</Label>
              <Input
                id="card-estimate"
                name="estimate"
                type="number"
                min={0}
                max={100000}
                step="any"
                defaultValue={card.estimate ?? ""}
                aria-describedby="card-estimate-hint"
              />
              <p id="card-estimate-hint" className="text-xs text-muted-foreground">
                {t("card-field-estimate-hint")}
              </p>
            </div>
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

/** The markdown renderer is its own chunk, loaded the first time a description is shown. */
const Markdown = lazy(() => import("@/components/markdown"));

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
