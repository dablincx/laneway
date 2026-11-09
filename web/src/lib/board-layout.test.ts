import { describe, expect, test } from "vitest";

import { layoutOf, moveCard, moveColumn, placeOf } from "./board-layout";

const layout = layoutOf(
  [{ id: "todo" }, { id: "doing" }, { id: "done" }],
  [
    { id: "a", column_id: "todo" },
    { id: "b", column_id: "todo" },
    { id: "c", column_id: "doing" },
  ],
);

describe("board layout", () => {
  test("built from the server lists, empty columns included", () => {
    expect(layout).toEqual({
      columns: ["todo", "doing", "done"],
      cards: { todo: ["a", "b"], doing: ["c"], done: [] },
    });
  });

  test("place of a card", () => {
    expect(placeOf(layout, "b")).toEqual({ columnId: "todo", index: 1 });
    expect(placeOf(layout, "nope")).toBeNull();
  });

  test("move a card within its column", () => {
    expect(moveCard(layout, "b", "todo", 0).cards.todo).toEqual(["b", "a"]);
  });

  test("move a card to another column, also an empty one and past the end", () => {
    const moved = moveCard(layout, "a", "doing", 0);
    expect(moved.cards).toEqual({ todo: ["b"], doing: ["a", "c"], done: [] });
    expect(moveCard(layout, "a", "done", 99).cards.done).toEqual(["a"]);
  });

  test("moving doesn't change the original", () => {
    moveCard(layout, "a", "doing", 0);
    expect(layout.cards.todo).toEqual(["a", "b"]);
  });

  test("unknown target column changes nothing", () => {
    expect(moveCard(layout, "a", "nope", 0)).toBe(layout);
  });

  test("move a column", () => {
    expect(moveColumn(layout, "done", 0).columns).toEqual(["done", "todo", "doing"]);
    expect(moveColumn(layout, "todo", 99).columns).toEqual(["doing", "done", "todo"]);
    expect(moveColumn(layout, "nope", 0)).toBe(layout);
  });
});
