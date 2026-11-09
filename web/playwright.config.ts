import { defineConfig, devices } from "@playwright/test";

// End-to-end tests against a running laneway. scripts/e2e.sh starts one (Vite + cargo locally, the
// Docker image in CI) and sets these env vars; e2e/env.ts says how the setup token is read.
const baseURL = process.env.LANEWAY_E2E_BASE_URL ?? "http://localhost:3101";

export default defineConfig({
  testDir: "e2e",
  testMatch: "**/*.e2e.ts",
  // The tests share one instance and build on each other (setup first), so one at a time.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL,
    locale: "en-US",
    trace: "off",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
