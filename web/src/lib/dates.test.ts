import { describe, expect, test } from "vitest";

import { formatDay, fromDateInput, isOverdue, toDateInput } from "./dates";

describe("card dates", () => {
  test("a picked day round-trips through the stored instant", () => {
    for (const day of ["2026-03-10", "2026-12-31", "2027-01-01", "2028-02-29"]) {
      const iso = fromDateInput(day);
      expect(iso).not.toBeNull();
      expect(toDateInput(iso)).toBe(day);
    }
  });

  test("empty and broken input mean no date", () => {
    expect(fromDateInput("")).toBeNull();
    expect(fromDateInput("10.03.2026")).toBeNull();
    expect(toDateInput(null)).toBe("");
    expect(toDateInput("not a date")).toBe("");
  });

  test("overdue only from the day after the due day", () => {
    const now = new Date(2026, 2, 10, 15, 0);
    expect(isOverdue(fromDateInput("2026-03-09") ?? "", now)).toBe(true);
    expect(isOverdue(fromDateInput("2026-03-10") ?? "", now)).toBe(false);
    expect(isOverdue(fromDateInput("2026-03-11") ?? "", now)).toBe(false);
  });

  test("formatted for the locale", () => {
    const iso = fromDateInput("2026-03-10") ?? "";
    expect(formatDay(iso, "en")).toContain("2026");
    expect(formatDay(iso, "de")).toContain("10.");
  });
});
