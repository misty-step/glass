import { expect, test } from "@playwright/test";

async function setSystemMode(page) {
  await page.addInitScript(() => {
    try {
      localStorage.setItem("ae-mode", "system");
    } catch (e) {}
  });
}

async function expectSharedRail(page, activeName: string | null) {
  const shell = page.locator(".ae-shell");
  const rail = page.locator(".ae-rail");
  const desk = page.locator(".ae-desk");
  await expect(shell).toHaveCount(1);
  await expect(rail).toHaveCount(1);
  await expect(desk).toHaveCount(1);

  await expect(rail.locator(".ae-logo")).toContainText("Glass");
  await expect(rail.locator(".ae-h")).toHaveText("PLACES");
  await expect(rail.getByRole("link", { name: "Now" })).toHaveAttribute(
    "href",
    "/",
  );
  await expect(
    rail.getByRole("link", { name: "Needs you · 2" }),
  ).toHaveAttribute("href", "/needs-you");
  await expect(rail.getByRole("link", { name: "Reports" })).toHaveAttribute(
    "href",
    "/rep1",
  );
  await expect(rail.getByRole("link", { name: "Clips" })).toHaveAttribute(
    "href",
    "/clips",
  );
  await expect(rail.locator("[data-sanctum-home]")).toContainText("Sanctum");
  await expect(
    rail.getByRole("link", { name: "Wire an agent" }),
  ).toHaveAttribute("href", "/setup");

  const active = rail.locator('[aria-current="page"]');
  if (activeName) {
    await expect(active).toHaveCount(1);
    await expect(active).toHaveText(activeName);
  } else {
    await expect(active).toHaveCount(0);
  }

  const mode = rail.locator(".ae-mode");
  await expect(mode).toHaveAttribute("data-mode", "system");
  await mode.click();
  await expect(mode).toHaveAttribute("data-mode", "dark");
  await expect(page.locator("html")).toHaveClass(/dark/);
}

test("viewer renders seeded posts, theme modes, and sandboxed iframe", async ({
  page,
  request,
}) => {
  await setSystemMode(page);
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
  await setSystemMode(page);
  await page.goto("/backlog/glass");
  await expectSharedRail(page, null);

  await expect(page.locator("#backlog-repo")).toHaveValue("glass");
  await expect(page.getByText("glass backlog")).toBeVisible();
  await expect(page.getByText("glass-905")).toBeVisible();
  await expect(page.getByText("Application floor")).toBeVisible();
  await expect(page.locator('[data-glance-component="table"]')).toBeVisible();
});

test("shared shell rail renders on every human HTML route", async ({ page }) => {
  await setSystemMode(page);

  const routes: Array<[string, string | null]> = [
    ["/", "Now"],
    ["/agent/e2e-agent", "Now"],
    ["/rep1", "Reports"],
    ["/clips", "Clips"],
    ["/backlog/glass", null],
    ["/needs-you", "Needs you · 2"],
    ["/review/sample", null],
  ];

  for (const [path, active] of routes) {
    await page.goto(path);
    await expectSharedRail(page, active);
  }

  await page.goto("/");
  const detailHref = await page
    .locator(".glass-feed-row", { hasText: "Rendered e2e seed" })
    .getByRole("link", { name: "post detail" })
    .getAttribute("href");
  expect(detailHref).toBeTruthy();
  await page.goto(detailHref!);
  await expectSharedRail(page, "Now");
});

test("shared rail becomes bottom chrome at 390px", async ({ page }) => {
  await setSystemMode(page);
  await page.setViewportSize({ width: 390, height: 760 });
  await page.goto("/needs-you");

  await expectSharedRail(page, "Needs you · 2");
  const railBox = await page.locator(".ae-rail").boundingBox();
  const deskBox = await page.locator(".ae-desk").boundingBox();
  expect(railBox).toBeTruthy();
  expect(deskBox).toBeTruthy();
  expect(railBox!.y).toBeGreaterThan(deskBox!.y);
  expect(Math.round(railBox!.width)).toBe(390);
  expect(railBox!.height).toBeLessThan(90);

  const scroll = await page.evaluate(() => ({
    body: document.body.scrollWidth,
    doc: document.documentElement.scrollWidth,
    win: window.innerWidth,
  }));
  expect(scroll.doc).toBeLessThanOrEqual(scroll.win);
  expect(scroll.body).toBeLessThanOrEqual(scroll.win);
});
