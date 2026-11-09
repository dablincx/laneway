import { expect, type Page, test } from "@playwright/test";

import { setupToken } from "./env";
import { text } from "./i18n";

// Runs against a fresh instance: the first tests need the setup page, later ones the admin it creates.
test.describe.configure({ mode: "serial" });

// Test-only account on a throwaway database.
const admin = {
  name: "Ada Admin",
  email: "ada@example.com",
  username: "ada",
  password: "e2e-password-not-a-secret",
};
const wrongPassword = "wrong-password-123";

const en = (id: string, args?: Record<string, string>) => text("en", id, args);

async function fillSetup(page: Page, token: string) {
  await page.locator("#setup_token").fill(token);
  await page.locator("#name").fill(admin.name);
  await page.locator("#email").fill(admin.email);
  await page.locator("#username").fill(admin.username);
  await page.locator("#password").fill(admin.password);
  await page.getByRole("button", { name: en("setup-submit") }).click();
}

async function logIn(page: Page, login: string, password: string, submit = en("auth-login-submit")) {
  await page.goto("/login");
  await page.locator("#login").fill(login);
  await page.locator("#password").fill(password);
  await page.getByRole("button", { name: submit }).click();
}

test("setup rejects a wrong token", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/setup$/);
  await expect(page.getByRole("heading", { name: en("setup-heading") })).toBeVisible();
  await fillSetup(page, "not-the-setup-token");
  await expect(page.getByRole("alert")).toHaveText(en("error-invalid-setup-token"));
  await expect(page).toHaveURL(/\/setup$/);
});

test("setup with the token from the log creates the admin", async ({ page }) => {
  await page.goto("/setup");
  await fillSetup(page, setupToken());
  await expect(page.getByText(en("home-welcome", { name: admin.name }))).toBeVisible();
  await expect(page).toHaveURL(/\/$/);
});

test("setup page is gone once the admin exists", async ({ page }) => {
  await page.goto("/setup");
  await expect(page).toHaveURL(/\/login$/);
});

test("log in with the username", async ({ page }) => {
  await logIn(page, admin.username, admin.password);
  await expect(page.getByText(en("home-welcome", { name: admin.name }))).toBeVisible();
});

test("log in with the email", async ({ page }) => {
  await logIn(page, admin.email, admin.password);
  await expect(page.getByText(en("home-welcome", { name: admin.name }))).toBeVisible();
});

test("wrong password shows an error", async ({ page }) => {
  await logIn(page, admin.username, wrongPassword);
  await expect(page.getByRole("alert")).toHaveText(en("error-invalid-credentials"));
  await expect(page).toHaveURL(/\/login$/);
});

test("log out ends the session", async ({ page }) => {
  await logIn(page, admin.username, admin.password);
  await page.getByRole("button", { name: en("auth-logout") }).click();
  await expect(page).toHaveURL(/\/login$/);
  // The session is really gone on the server, not just the cached user.
  await page.reload();
  await expect(page).toHaveURL(/\/login$/);
  await page.goto("/");
  await expect(page).toHaveURL(/\/login$/);
});

test.describe("german ui", () => {
  test.use({ locale: "de-DE" });
  const de = (id: string, args?: Record<string, string>) => text("de", id, args);

  test("login, error and home page are in german", async ({ page }) => {
    await page.goto("/login");
    await expect(page.getByRole("heading", { name: de("auth-login-heading") })).toBeVisible();
    await expect(page.getByLabel(de("auth-login-field-login"))).toBeVisible();

    await logIn(page, admin.username, wrongPassword, de("auth-login-submit"));
    await expect(page.getByRole("alert")).toHaveText(de("error-invalid-credentials"));

    await logIn(page, admin.username, admin.password, de("auth-login-submit"));
    await expect(page.getByText(de("home-welcome", { name: admin.name }))).toBeVisible();
    await expect(page.getByRole("button", { name: de("auth-logout") })).toBeVisible();
  });
});
