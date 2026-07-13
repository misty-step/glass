import { defineConfig } from "@playwright/test";

const glassPort = process.env.GLASS_E2E_PORT || "19041";
const powderPort = process.env.GLASS_E2E_POWDER_PORT || "19042";
const glassBaseUrl =
  process.env.GLASS_E2E_BASE_URL || `http://127.0.0.1:${glassPort}`;
const powderBaseUrl = `http://127.0.0.1:${powderPort}`;

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  expect: {
    timeout: 10_000,
  },
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  reporter: [
    ["list"],
    ["junit", { outputFile: "target/e2e/junit.xml" }],
    ["html", { outputFolder: "target/e2e/playwright-report", open: "never" }],
  ],
  outputDir: "target/e2e/test-results",
  use: {
    baseURL: glassBaseUrl,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  webServer: [
    {
      command: `GLASS_E2E_POWDER_PORT=${powderPort} node tests/e2e/support/mock-powder.mjs`,
      url: `${powderBaseUrl}/health`,
      reuseExistingServer: false,
      timeout: 120_000,
    },
    {
      command: `GLASS_E2E_PORT=${glassPort} GLASS_POWDER_API_BASE_URL=${powderBaseUrl} GLASS_POWDER_API_KEY=e2e GLASS_BITTERBLOSSOM_API_BASE_URL=${powderBaseUrl} GLASS_BITTERBLOSSOM_API_KEY=e2e ./scripts/e2e-server.sh`,
      url: `${glassBaseUrl}/api/surface-kinds`,
      reuseExistingServer: false,
      timeout: 120_000,
    },
  ],
});
