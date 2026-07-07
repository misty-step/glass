import { expect, test } from "@playwright/test";

test("viewer renders seeded posts, theme modes, and sandboxed iframe", async ({
  page,
  request,
}) => {
  await page.goto("/");

  const seedRow = page.locator(".glass-feed-row", {
    hasText: "Rendered e2e seed",
  });
  await expect(seedRow).toHaveCount(1);
  await expect(seedRow.locator(".glass-feed-chip")).toHaveText("report");
  await expect(seedRow.locator(".glass-feed-title")).toHaveText(
    "Rendered e2e seed",
  );
  await expect(seedRow.locator(".glass-feed-summary")).toHaveText(
    "3 surface(s): html, metric, json",
  );
  await expect(seedRow.locator(".glass-feed-meta")).toContainText("e2e-agent");
  await expect(seedRow.locator(".glass-feed-meta")).toContainText(
    "Rendered e2e lane",
  );

  await seedRow.getByRole("link", { name: "post detail" }).click();
  await expect(page).toHaveURL(/\/session\/[^/]+\/p\/[^/]+$/);

  await expect(page.locator(".glass-post-title")).toHaveText("Rendered e2e seed");
  await expect(page.locator(".glass-post-meta").first()).toContainText(
    "e2e-agent",
  );
  await expect(page.locator(".glass-metric-label")).toHaveText("seed");
  await expect(page.locator(".glass-metric-value")).toHaveText("green");
  await expect(page.locator(".glass-surface pre")).toContainText(
    '"fixture": "trusted-viewer"',
  );

  const mode = page.locator(".ae-mode");
  await expect(mode).toHaveAttribute("data-mode", "system");
  await expect(page.locator("html")).not.toHaveClass(/dark|light/);

  await mode.click();
  await expect(mode).toHaveAttribute("data-mode", "dark");
  await expect(page.locator("html")).toHaveClass(/dark/);
  expect(await page.evaluate(() => localStorage.getItem("ae-mode"))).toBe("dark");

  await mode.click();
  await expect(mode).toHaveAttribute("data-mode", "light");
  await expect(page.locator("html")).toHaveClass(/light/);
  expect(await page.evaluate(() => localStorage.getItem("ae-mode"))).toBe(
    "light",
  );

  await mode.click();
  await expect(mode).toHaveAttribute("data-mode", "system");
  await expect(page.locator("html")).not.toHaveClass(/dark|light/);
  expect(await page.evaluate(() => localStorage.getItem("ae-mode"))).toBe(
    "system",
  );

  const iframe = page.frameLocator('iframe[src^="/s/"]').first();
  await expect(iframe.locator("#sandbox-proof")).toHaveText("Sandbox Proof");
  await iframe.locator("#sandbox-button").click();
  await expect(iframe.locator("body")).toHaveAttribute("data-clicked", "yes");

  const src = await page.locator('iframe[src^="/s/"]').first().getAttribute("src");
  expect(src).toBeTruthy();
  const sandboxResponse = await request.get(src!);
  expect(sandboxResponse.ok()).toBe(true);
  const csp = sandboxResponse.headers()["content-security-policy"] ?? "";
  expect(csp).toContain("sandbox");
  expect(csp).not.toContain("allow-same-origin");
});

test("native backlog surface renders through the shared report path", async ({
  page,
}) => {
  await page.goto("/backlog/glass");

  await expect(page.locator("#backlog-repo")).toHaveValue("glass");
  await expect(page.getByText("glass backlog")).toBeVisible();
  await expect(page.getByText("glass-905")).toBeVisible();
  await expect(page.getByText("Application floor")).toBeVisible();
  await expect(page.locator('[data-glance-component="table"]')).toBeVisible();
});
