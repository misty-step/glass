import { expect, test } from "@playwright/test";

const powderBaseUrl = `http://127.0.0.1:${process.env.GLASS_E2E_POWDER_PORT || "19042"}`;

test.beforeEach(async () => {
  await fetch(`${powderBaseUrl}/__reset`, { method: "POST" });
});

async function setSystemMode(page) {
  await page.addInitScript(() => {
    try {
      localStorage.setItem("ae-mode", "system");
    } catch (e) {}
  });
}

async function expectSharedRail(page, activeName: string | null) {
  const needsYouName = activeName?.startsWith("Needs you")
    ? activeName
    : "Needs you · 2";
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
  await expect(rail.getByRole("link", { name: needsYouName })).toHaveAttribute(
    "href",
    "/needs-you",
  );
  await expect(rail.getByRole("link", { name: "Reports" })).toHaveAttribute(
    "href",
    "/reports",
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
  const modeState = await mode.getAttribute("data-mode");
  expect(["system", "dark", "light"]).toContain(modeState);
  if (modeState === "system") {
    await mode.click();
    await expect(mode).toHaveAttribute("data-mode", "dark");
    await expect(page.locator("html")).toHaveClass(/dark/);
  }
}

test("viewer renders seeded posts, theme modes, and sandboxed iframe", async ({
  page,
  request,
}) => {
  await setSystemMode(page);
  await page.goto("/");

  await expect(page.locator(".ae-stat-badges")).toBeVisible();
  await expect(page.locator(".ae-stat-badge", { hasText: "agents live" })).toContainText("2");
  await expect(page.locator(".ae-stat-badge", { hasText: "need you" })).toContainText("2");

  const richCard = page.locator(".ae-wall-card", { hasText: "e2e-agent" });
  await expect(richCard).toHaveCount(1);
  await expect(richCard).toContainText("powder glass-932");
  await expect(richCard).toContainText("Rendered e2e seed");

  const quietCard = page.locator(".ae-wall-card.mk-quiet-card", {
    hasText: "quiet-agent",
  });
  await expect(quietCard).toHaveCount(1);
  await expect(quietCard).toContainText("powder glass-quiet");
  await expect(quietCard).toContainText("no posts yet");

  const seedRow = page.locator(".ae-list-row", {
    hasText: "Rendered e2e seed",
  });
  await expect(seedRow).toHaveCount(1);
  await expect(seedRow.locator(".ae-chip")).toHaveText("report");
  await expect(seedRow).toContainText("e2e-agent");

  await seedRow.click();
  await expect(page.locator("#feed-dialog")).toBeVisible();
  await expect(page.locator("#feed-dialog")).toContainText("Rendered e2e seed");
  await page.locator("[data-feed-close]").click();

  const detailHref = await seedRow.getAttribute("href");
  expect(detailHref).toBeTruthy();
  await page.goto(detailHref!);

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

test("now teaches the empty fleet and empty wire states", async ({ page }) => {
  await setSystemMode(page);
  await page.route("**/api/now", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        stats: {
          agentsLive: 0,
          needYouCount: 0,
          postsToday: 0,
          sessionsToday: 0,
          secondsSinceLastEvent: null,
        },
        wall: [],
        wire: [],
        dead: { agentCount: 0, sessionCount: 0, sessions: [] },
        notices: [],
        landmark: {
          status: "unconfigured",
          message: "GLASS_LANDMARK_RELEASE_EVENTS_URL is not configured",
        },
      }),
    });
  });

  await page.goto("/");
  await expect(page.locator(".glass-wall-empty")).toContainText(
    "Nothing on stage. Agents appear here when they claim a Powder card or publish",
  );
  await expect(
    page.locator(".glass-wall-empty").getByRole("link", {
      name: "Wire an agent",
    }),
  ).toHaveAttribute("href", "/setup");
  await expect(page.locator(".glass-wire-empty")).toContainText(
    "No ambient evidence yet. Landmark: unconfigured",
  );
});

test("reports generator persists a last-week fleet activity digest", async ({
  page,
}) => {
  await setSystemMode(page);
  await page.goto("/reports");
  await expectSharedRail(page, "Reports");

  await expect(page.getByText("GENERATE A REPORT")).toBeVisible();
  await expect(page.locator("#reports-range")).toContainText("->");
  await page.getByRole("button", { name: "Generate report" }).click();
  await expect(page).toHaveURL(/\/reports\/R-001$/);
  await expectSharedRail(page, "Reports");
  await expect(page.getByText("Activity digest - fleet").first()).toBeVisible();
  await expect(page.getByText("Native service MVP")).toBeVisible();
  await expect(page.getByText("Powder completions")).toBeVisible();
  await expect(
    page
      .locator('[data-glance-component="table"]')
      .filter({ hasText: "Powder completions" }),
  ).toBeVisible();
});

test("shared shell rail renders on every human HTML route", async ({ page }) => {
  await setSystemMode(page);

  const routes: Array<[string, string | null]> = [
    ["/", "Now"],
    ["/agent/e2e-agent", "Now"],
    ["/reports", "Reports"],
    ["/clips", "Clips"],
    ["/needs-you", "Needs you · 2"],
    ["/review/sample", null],
  ];

  for (const [path, active] of routes) {
    await page.goto(path);
    await expectSharedRail(page, active);
  }

  await page.goto("/");
  const detailHref = await page
    .locator(".ae-list-row", { hasText: "Rendered e2e seed" })
    .getAttribute("href");
  expect(detailHref).toBeTruthy();
  await page.goto(detailHref!);
  await expectSharedRail(page, "Now");
});

test("needs-you renders mock Powder asks and relays an answer", async ({
  page,
}) => {
  await setSystemMode(page);
  await page.goto("/needs-you");
  await expectSharedRail(page, "Needs you · 2");

  await expect(page.locator("#ny-body > .ae-h")).toHaveText(
    "WAITING ON YOU · 2",
  );
  await expect(page.locator(".ny-row")).toHaveCount(2);
  await expect(page.locator(".ny-row").first().locator(".ae-item")).toHaveText(
    "DECIDE: keep the rail active on viewer drill-downs?",
  );
  await expect(page.locator(".ny-row").first().locator(".ae-dim")).toContainText(
    "glass-931-codex · powder glass-931 · asked",
  );

  await page.locator(".ny-row").first().getByRole("button", { name: "Answer" }).click();
  const dialog = page.locator("#ny-dialog");
  await expect(dialog).toBeVisible();
  await dialog.locator("textarea").fill("Keep it active.");
  await dialog.getByRole("button", { name: "Answer" }).click();

  await expect(dialog).toBeHidden();
  await expect(page.locator("#ny-body > .ae-h")).toHaveText(
    "WAITING ON YOU · 1",
  );
  await expectSharedRail(page, "Needs you · 1");
  await expect(page.locator("details.ae-fold")).toContainText("ANSWERED");
});

test("clips renders in the shared shell with honest empty capture guidance", async ({
  page,
}) => {
  await setSystemMode(page);
  await page.goto("/clips");
  await expectSharedRail(page, "Clips");
  await expect(page.getByText("Clip review queue")).toBeVisible();
  await expect(page.getByText("No clips captured yet")).toBeVisible();
  await expect(page.getByText("MCP capture_clip")).toBeVisible();
  await expect(page.getByText("POST /api/clips")).toBeVisible();
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
