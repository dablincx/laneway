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
