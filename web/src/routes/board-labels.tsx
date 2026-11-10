import { Check, Tags, Trash2 } from "lucide-react";
import { useState } from "react";

import { boardApi, type Label, type LabelColor, labelColors } from "@/api/boards";
import { errorMessage } from "@/api/errors";
import { FormError } from "@/components/form";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import { useBoardMutation } from "@/lib/use-board-mutation";

/** Spelled out so the translation check can see every key in use. */
const colorName: Record<LabelColor, string> = {
  gray: "label-color-gray",
  red: "label-color-red",
  orange: "label-color-orange",
  yellow: "label-color-yellow",
  green: "label-color-green",
  teal: "label-color-teal",
  blue: "label-color-blue",
  purple: "label-color-purple",
  pink: "label-color-pink",
};

// Full class names (not built from the color), so Tailwind keeps them in the build.
const chipStyle: Record<LabelColor, string> = {
  gray: "bg-zinc-500/15 text-zinc-700 dark:text-zinc-300",
  red: "bg-red-500/15 text-red-700 dark:text-red-300",
  orange: "bg-orange-500/15 text-orange-700 dark:text-orange-300",
  yellow: "bg-yellow-500/20 text-yellow-800 dark:text-yellow-200",
  green: "bg-green-500/15 text-green-700 dark:text-green-300",
  teal: "bg-teal-500/15 text-teal-700 dark:text-teal-300",
  blue: "bg-blue-500/15 text-blue-700 dark:text-blue-300",
  purple: "bg-purple-500/15 text-purple-700 dark:text-purple-300",
  pink: "bg-pink-500/15 text-pink-700 dark:text-pink-300",
};

export function LabelChip({ label }: { label: Label }) {
  return (
    <span
      className={`inline-flex max-w-full truncate rounded px-1.5 text-xs font-medium ${chipStyle[label.color]}`}
    >
      {label.name}
    </span>
  );
}

function ColorSelect({
  id,
  value,
  onChange,
  name,
}: {
  id: string;
  value?: LabelColor;
  onChange?: (color: LabelColor) => void;
  name?: string;
}) {
  const { t } = useI18n();
  return (
    <select
      id={id}
      name={name}
      aria-label={t("label-field-color")}
      className="h-9 rounded-md border bg-transparent px-2 text-sm"
      value={value}
      defaultValue={value === undefined ? "blue" : undefined}
      onChange={(e) => onChange?.(e.target.value as LabelColor)}
    >
      {labelColors.map((c) => (
        <option key={c} value={c}>
          {t(colorName[c])}
        </option>
      ))}
    </select>
  );
}

/** Create, rename, recolor and delete a board's labels. */
export function LabelsDialog({
  boardId,
  labels,
  onClose,
}: {
  boardId: string;
  labels: Label[];
  onClose: () => void;
}) {
  const { t } = useI18n();
  const create = useBoardMutation(boardId, (name: string, color: LabelColor) =>
    boardApi.createLabel(boardId, name, color),
  );
  const update = useBoardMutation(boardId, (id: string, changes: { name?: string; color?: LabelColor }) =>
    boardApi.updateLabel(id, changes),
  );
  const remove = useBoardMutation(boardId, (id: string) => boardApi.deleteLabel(id));
  const error = [create, update, remove].find((m) => m.isError)?.error;

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent aria-describedby={undefined}>
        <DialogTitle>{t("label-manage-heading")}</DialogTitle>
        {labels.length === 0 && <p className="text-sm text-muted-foreground">{t("label-none")}</p>}
        <ul className="flex flex-col gap-2">
          {labels.map((label) => (
            <li key={label.id} className="flex items-center gap-2">
              <Input
                aria-label={t("label-field-name")}
                defaultValue={label.name}
                className="flex-1"
                onBlur={(e) => {
                  const name = e.currentTarget.value.trim();
                  if (name && name !== label.name) update.mutate([label.id, { name }]);
                }}
                onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              />
              <ColorSelect
                id={`label-color-${label.id}`}
                value={label.color}
                onChange={(color) => update.mutate([label.id, { color }])}
              />
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("label-delete", { name: label.name })}
                onClick={() => remove.mutate([label.id])}
              >
                <Trash2 className="size-4" />
              </Button>
            </li>
          ))}
        </ul>
        <form
          className="flex items-center gap-2 border-t pt-4"
          onSubmit={(e) => {
            e.preventDefault();
            const form = e.currentTarget;
            const data = new FormData(form);
            create.mutate([String(data.get("name") ?? ""), String(data.get("color")) as LabelColor], {
              onSuccess: () => form.reset(),
            });
          }}
        >
          <Input
            name="name"
            aria-label={t("label-new-name")}
            placeholder={t("label-new-name")}
            required
            className="flex-1"
          />
          <ColorSelect id="label-new-color" name="color" />
          <Button type="submit" disabled={create.isPending}>
            {t("label-new-submit")}
          </Button>
        </form>
        <FormError message={error ? errorMessage(t, error) : null} />
      </DialogContent>
    </Dialog>
  );
}

/** Toggles the board's labels on one card; each click saves right away. */
export function LabelPicker({
  boardId,
  cardId,
  labels,
  selected,
  onManage,
}: {
  boardId: string;
  cardId: string;
  labels: Label[];
  selected: string[];
  onManage: () => void;
}) {
  const { t } = useI18n();
  // Shown right away, saved in the background.
  const [current, setCurrent] = useState(selected);
  const save = useBoardMutation(boardId, (ids: string[]) => boardApi.setCardLabels(cardId, ids));
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-2 text-sm font-medium">{t("card-field-labels")}</legend>
      <div className="flex flex-wrap items-center gap-1.5">
        {labels.map((label) => {
          const on = current.includes(label.id);
          return (
            <button
              key={label.id}
              type="button"
              aria-pressed={on}
              className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs font-medium ${chipStyle[label.color]} ${
                on ? "ring-2 ring-ring" : "opacity-60 hover:opacity-100"
              }`}
              onClick={() => {
                const next = on ? current.filter((id) => id !== label.id) : [...current, label.id];
                setCurrent(next);
                save.mutate([next]);
              }}
            >
              {on && <Check className="size-3" aria-hidden />}
              {label.name}
            </button>
          );
        })}
        <Button type="button" variant="ghost" size="sm" onClick={onManage}>
          <Tags className="size-4" aria-hidden />
          {t("label-manage")}
        </Button>
      </div>
      <FormError message={save.isError ? errorMessage(t, save.error) : null} />
    </fieldset>
  );
}
