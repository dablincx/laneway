import { expect, test } from "@playwright/test";

import { text } from "./i18n";

// One worker runs the files in alphabetical order, so this runs after auth.e2e.ts and logs in
// with the admin that file created.
test.describe.configure({ mode: "serial" });

const en = (id: string) => text("en", id);
const de = (id: string) => text("de", id);
const login = { username: "ada", password: "e2e-password-not-a-secret" };

async function logIn(page: import("@playwright/test").Page) {
  await page.goto("/login");
  await page.locator("#login").fill(login.username);
  await page.locator("#password").fill(login.password);
  await page.getByRole("button", { name: en("auth-login-submit") }).click();
  await expect(page.getByRole("heading", { name: en("board-list-heading") })).toBeVisible();
}

test("create a board, add cards, move one to done", async ({ page }) => {
  await logIn(page);
  await page.getByLabel(en("board-new-name")).fill("e2e groceries");
  await page.getByRole("button", { name: en("board-new-submit") }).click();
  await expect(page).toHaveURL(/\/boards\/[0-9a-f-]+$/);

  const todo = page.getByRole("region", { name: en("board-default-column-todo") });
  const done = page.getByRole("region", { name: en("board-default-column-done") });
  for (const title of ["milk", "bread"]) {
    await todo.getByLabel(en("card-new-title")).fill(title);
    await todo.getByLabel(en("card-new-title")).press("Enter");
    await expect(todo.getByRole("button", { name: title })).toBeVisible();
  }

  await todo.getByRole("button", { name: "bread" }).click();
  await page
    .getByLabel(en("card-field-column"), { exact: true })
    .selectOption({ label: en("board-default-column-done") });
  await page.getByLabel(en("card-field-description"), { exact: true }).fill("whole grain");
  await page.getByRole("button", { name: en("common-save") }).click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect(done.getByRole("button", { name: "bread" })).toBeVisible();

  // Survives a reload: it's all on the server.
  await page.reload();
  await expect(done.getByRole("button", { name: "bread" })).toBeVisible();
  await expect(todo.getByRole("button", { name: "milk" })).toBeVisible();
  await done.getByRole("button", { name: "bread" }).click();
  await expect(page.getByLabel(en("card-field-description"), { exact: true })).toHaveValue("whole grain");
});

async function newBoard(page: import("@playwright/test").Page, name: string, cards: string[]) {
  await logIn(page);
  await page.getByLabel(en("board-new-name")).fill(name);
  await page.getByRole("button", { name: en("board-new-submit") }).click();
  const todo = page.getByRole("region", { name: en("board-default-column-todo") });
  for (const title of cards) {
    await todo.getByLabel(en("card-new-title")).fill(title);
    await todo.getByLabel(en("card-new-title")).press("Enter");
    await expect(todo.getByRole("button", { name: title })).toBeVisible();
  }
  return {
    todo,
    doing: page.getByRole("region", { name: en("board-default-column-doing") }),
    done: page.getByRole("region", { name: en("board-default-column-done") }),
  };
}

type Locator = import("@playwright/test").Locator;

/**
 * Drags `item` with the keyboard: space, the arrow keys, space. Each step names the target it
 * should be over and, when the item changes columns, where it shows up before the next key
 * (people press keys slower than tests; the board needs a moment to measure the new spot).
 */
async function keyDrag(
  page: import("@playwright/test").Page,
  item: string,
  steps: [string, string, Locator?][],
) {
  // Each step waits for the screen reader announcement, which also checks they are there.
  const said = (id: string, args: Record<string, string>) =>
    expect(page.getByText(text("en", id, args), { exact: true })).toBeAttached();
  await page.keyboard.press("Space");
  await said("dnd-picked-up", { item });
  // dnd-kit starts listening for the arrow keys one task after the pick-up (a setTimeout);
  // a key pressed before that is lost. People are never that fast, tests against a
  // production build are. Waiting for one task in the page lets dnd-kit's timer run first.
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 0)));
  for (const [key, target, landed] of steps) {
    await page.keyboard.press(key);
    await said("dnd-over", { item, target });
    if (landed) await expect(landed).toBeVisible();
  }
  await page.keyboard.press("Space");
}

test("drag a card to another column with the mouse", async ({ page }) => {
  const { todo, doing } = await newBoard(page, "e2e drag mouse", ["wash car"]);
  const card = todo.getByRole("button", { name: "wash car" });
  const from = await card.boundingBox();
  const to = await doing.boundingBox();
  if (!from || !to) throw new Error("no layout");
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  // A few steps, so the drag starts (5px threshold) and the columns see the card coming.
  await page.mouse.move(from.x + 40, from.y + 10, { steps: 5 });
  await page.mouse.move(to.x + to.width / 2, to.y + 40, { steps: 10 });
  await page.mouse.up();
  await expect(doing.getByRole("button", { name: "wash car" })).toBeVisible();
  await page.reload();
  await expect(doing.getByRole("button", { name: "wash car" })).toBeVisible();
});

test("reorder cards and move them across columns with the keyboard", async ({ page }) => {
  const { todo, doing, done } = await newBoard(page, "e2e drag keys", ["first", "second"]);
  // Space picks up, arrows move, space drops.
  await todo.getByRole("button", { name: "second" }).focus();
  await keyDrag(page, "second", [["ArrowUp", "first"]]);
  await expect(todo.getByRole("listitem").first()).toHaveText("second");

  await todo.getByRole("button", { name: "first" }).focus();
  await keyDrag(page, "first", [
    ["ArrowRight", en("board-default-column-doing"), doing.getByRole("button", { name: "first" })],
    ["ArrowRight", en("board-default-column-done"), done.getByRole("button", { name: "first" })],
  ]);
  // The done column marks it as done.
  await expect(done.getByRole("button", { name: "first" })).toBeVisible();
  await expect(done.getByLabel(en("card-done"))).toBeVisible();
  await page.reload();
  await expect(done.getByRole("button", { name: "first" })).toBeVisible();
  await expect(todo.getByRole("listitem")).toHaveText(["second"]);
});

test("reorder columns with the keyboard", async ({ page }) => {
  await newBoard(page, "e2e drag columns", []);
  const names = () =>
    page.getByRole("region").evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  const done = page.getByRole("region", { name: en("board-default-column-done") });
  await done.getByRole("button", { name: en("column-drag") }).focus();
  await keyDrag(page, en("board-default-column-done"), [
    ["ArrowLeft", en("board-default-column-doing")],
    ["ArrowLeft", en("board-default-column-todo")],
  ]);
  const order = [
    en("board-default-column-done"),
    en("board-default-column-todo"),
    en("board-default-column-doing"),
  ];
  await expect.poll(names).toEqual(order);
  await page.reload();
  await expect(done).toBeVisible();
  await expect.poll(names).toEqual(order);
});

test("column settings: WIP limit and done column", async ({ page }) => {
  const { todo } = await newBoard(page, "e2e column settings", ["one", "two"]);
  const dialog = page.getByRole("dialog");
  await todo.getByRole("button", { name: en("column-settings") }).click();
  await dialog.getByLabel(en("column-field-wip-limit"), { exact: true }).fill("1");
  await dialog.getByLabel(en("column-field-done"), { exact: true }).check();
  await dialog.getByRole("button", { name: en("common-save") }).click();
  await expect(dialog).toBeHidden();
  await expect(todo.getByText("2/1")).toBeVisible();
  await expect(todo.getByLabel(en("column-field-done"))).toBeVisible();
  await expect(todo.getByTitle(en("column-over-limit"))).toBeVisible();

  // Removing the limit again.
  await todo.getByRole("button", { name: en("column-settings") }).click();
  await dialog.getByLabel(en("column-field-wip-limit"), { exact: true }).fill("");
  await dialog.getByRole("button", { name: en("common-save") }).click();
  await expect(todo.getByText("2/1")).toBeHidden();

  // A column with cards can't be deleted.
  await todo.getByRole("button", { name: en("column-settings") }).click();
  await expect(dialog.getByRole("button", { name: en("column-delete") })).toBeDisabled();
});

test.describe("german", () => {
  test.use({ locale: "de-DE" });

  test("new boards get german default columns", async ({ page }) => {
    await page.goto("/login");
    await page.locator("#login").fill(login.username);
    await page.locator("#password").fill(login.password);
    await page.getByRole("button", { name: de("auth-login-submit") }).click();
    await page.getByLabel(de("board-new-name")).fill("e2e arbeit");
    await page.getByRole("button", { name: de("board-new-submit") }).click();
    for (const id of [
      "board-default-column-todo",
      "board-default-column-doing",
      "board-default-column-done",
    ]) {
      await expect(page.getByRole("region", { name: de(id) })).toBeVisible();
    }
  });
});

test("unknown pages show a translated not-found page", async ({ page }) => {
  await logIn(page);
  await page.goto("/no-such-page");
  await expect(page.getByRole("heading", { name: en("common-not-found") })).toBeVisible();
  await page.getByRole("link", { name: en("common-back-home") }).click();
  await expect(page.getByRole("heading", { name: en("board-list-heading") })).toBeVisible();
});

test.describe("german not-found page", () => {
  test.use({ locale: "de-DE" });

  test("is translated", async ({ page }) => {
    await page.goto("/no-such-page");
    await expect(page.getByRole("heading", { name: de("common-not-found") })).toBeVisible();
  });
});

test("an expired session sends you to the login page", async ({ page }) => {
  await logIn(page);
  await page.getByLabel(en("board-new-name")).fill("e2e expiry");
  await page.getByRole("button", { name: en("board-new-submit") }).click();
  const todo = page.getByRole("region", { name: en("board-default-column-todo") });
  await expect(todo).toBeVisible();

  // The session ends while the page stays open (expired or logged out elsewhere).
  await page.context().clearCookies();
  await todo.getByLabel(en("card-new-title")).fill("after expiry");
  await todo.getByLabel(en("card-new-title")).press("Enter");
  await expect(page).toHaveURL(/\/login$/);
});
