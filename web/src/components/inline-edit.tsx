import { useState } from "react";

import { Input } from "@/components/ui/input";

/** Text that turns into an input on click and saves on Enter or blur. */
export function InlineEdit({
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
        if (e.key === "Enter") {
          // Inside a form (the card dialog) Enter would submit it too.
          e.preventDefault();
          finish(e.currentTarget.value);
        }
        if (e.key === "Escape") setEditing(false);
      }}
    />
  );
}
