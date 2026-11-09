import { defineConfig, devices } from "@playwright/test";

// End-to-end tests against a running laneway. scripts/e2e.sh starts one (Vite + cargo locally, the
// Docker image in CI) and sets these env vars; e2e/env.ts says how the setup token is read.
const baseURL = process.env.LANEWAY_E2E_BASE_URL ?? "http://localhost:3101";

// One browser per run, because the tests build on each other and need a fresh instance.
// CI runs the suite once for each of chromium, firefox and webkit.
const browsers = {
  chromium: devices["Desktop Chrome"],
  firefox: devices["Desktop Firefox"],
  webkit: devices["Desktop Safari"],
};
const browser = (process.env.LANEWAY_E2E_BROWSER ?? "chromium") as keyof typeof browsers;
if (!(browser in browsers)) throw new Error(`LANEWAY_E2E_BROWSER must be one of ${Object.keys(browsers).join(", ")}`);

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
  projects: [{ name: browser, use: { ...browsers[browser] } }],
});
