/**
 * Card dates are picked as calendar days in the viewer's time zone and stored as that day's
 * local midnight (an instant in UTC on the server).
 */

/** `YYYY-MM-DD` for a date input, in local time. Empty for no date. */
export function toDateInput(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The ISO instant of local midnight on a `YYYY-MM-DD` day, or `null` for an empty input. */
export function fromDateInput(value: string): string | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const [y, m, d] = match.slice(1).map(Number) as [number, number, number];
  return new Date(y, m - 1, d).toISOString();
}

/** True if the due day is before today (local time). Due today is not overdue. */
export function isOverdue(dueIso: string, now: Date = new Date()): boolean {
  return toDateInput(dueIso) < toDateInput(now.toISOString());
}

/** A short localized date like "10 Mar 2026". */
export function formatDay(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(new Date(iso));
}
