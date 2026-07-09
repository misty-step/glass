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
  const shell = page.locator(".glass-shell");
  const rail = page.locator(".glass-rail");
  const desk = page.locator(".glass-desk");
  await expect(shell).toHaveCount(1);
  await expect(rail).toHaveCount(1);
  await expect(desk).toHaveCount(1);

  await expect(rail.locator(".ae-logo")).toContainText("GLASS");
  await expect(rail.locator(".ae-h", { hasText: "PLACES" })).toHaveCount(1);
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
  await expect(rail.getByRole("link", { name: "Clips" })).toHaveCount(0);
  await expect(rail.locator("[data-sanctum-home]")).toContainText("Sanctum");
  await expect(
    rail.getByRole("link", { name: "Wire an agent" }),
  ).toHaveAttribute("href", "/setup");

  const active = rail.locator('[aria-current="page"]');
  if (activeName) {
    await expect(active).toHaveCount(1);
    await expect(active).toContainText(activeName);
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
  await expect(page.locator(".ae-stat-badge", { hasText: "working" })).toContainText("2");
  await expect(page.locator(".ae-stat-badge", { hasText: "need you" })).toContainText("2");

  const richCard = page.locator(".glass-now-row", { hasText: "e2e-agent" });
  await expect(richCard).toHaveCount(1);
  await expect(richCard).toContainText("powder glass-932");
  await expect(richCard).toContainText("Rendered e2e seed");

  const quietCard = page.locator(".glass-now-row.is-quiet", {
    hasText: "quiet-agent",
  });
  await expect(quietCard).toHaveCount(1);
  await expect(quietCard).toContainText("powder glass-quiet");
  await expect(quietCard).toContainText("quiet ·");

  const seedRow = page.locator(".glass-wire-tape-row", {
    hasText: "Rendered e2e seed",
  });
  await expect(seedRow).toHaveCount(1);
  await expect(seedRow.locator(".glass-wire-kind")).toHaveAttribute("title", "report");
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
  await expect(page.locator(".glass-now-empty")).toContainText(
    "Nothing on stage. Agents appear here when they claim a Powder card or publish",
  );
  await expect(
    page.locator(".glass-now-empty").getByRole("link", {
      name: "Wire an agent",
    }),
  ).toHaveAttribute("href", "/setup");
  await expect(page.locator(".glass-wire-empty")).toContainText(
    "No ambient evidence yet. Landmark: unconfigured",
  );
});

test("now renders the locked NOW-9 column and WIRE-10 tape", async ({
  page,
}) => {
  await setSystemMode(page);
  const now = Math.floor(Date.now() / 1000);
  await page.route("**/api/now", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        stats: {
          agentsLive: 3,
          needYouCount: 1,
          postsToday: 4,
          sessionsToday: 3,
          secondsSinceLastEvent: 12,
        },
        wall: [
          {
            agent: "alpha-live",
            href: "/agent/alpha-live",
            status: "ok",
            powderTag: "powder glass-live",
            powderCardId: "glass-live",
            powderTitle: "Live work",
            meta: "report: shaping the viewer",
            sessionId: "ses-live",
            sessionTitle: "live lane",
            postId: "post-live",
            latestKind: "report",
            latestAt: now - 20,
            ageSeconds: 20,
            claimedAt: now - 900,
            quiet: false,
            trace: [],
          },
          {
            agent: "m-quiet",
            href: "/agent/m-quiet",
            status: "quiet",
            powderTag: "powder glass-quiet",
            powderCardId: "glass-quiet",
            powderTitle: "Quiet work",
            meta: "claimed 1d ago · no posts yet",
            sessionId: null,
            sessionTitle: null,
            postId: null,
            latestKind: null,
            latestAt: null,
            ageSeconds: 86_400,
            claimedAt: now - 86_400,
            quiet: true,
            trace: [],
          },
          {
            agent: "z-blocked",
            href: "/agent/z-blocked",
            status: "warn",
            powderTag: "powder glass-blocked",
            powderCardId: "glass-blocked",
            powderTitle: "Blocked work",
            meta: "blocked: waiting for ingest key",
            sessionId: "ses-blocked",
            sessionTitle: "blocked lane",
            postId: "post-blocked",
            latestKind: "blocked",
            latestAt: now - 2_520,
            ageSeconds: 2_520,
            claimedAt: now - 3_000,
            quiet: false,
            trace: [],
          },
        ],
        wire: [
          {
            id: "wire-blocked",
            kind: "blocked",
            source: "glass",
            title: "Canary key blocks deploy",
            summary: "Needs a key",
            occurredAt: now - 40,
            agent: "z-blocked",
            sessionId: "ses-blocked",
            sessionTitle: "blocked lane",
            postId: "post-blocked",
            evidenceLinks: [],
            detailLines: ["blocks deploy"],
          },
          {
            id: "wire-question",
            kind: "question",
            source: "glass",
            title: "Choose report window",
            summary: "Needs a decision",
            occurredAt: now - 55,
            agent: "planner",
            sessionId: null,
            sessionTitle: null,
            postId: null,
            evidenceLinks: [],
            detailLines: [],
          },
          {
            id: "wire-report",
            kind: "report",
            source: "glass",
            title: "Report generated",
            summary: "Digest ready",
            occurredAt: now - 70,
            agent: "alpha-live",
            sessionId: "ses-live",
            sessionTitle: "live lane",
            postId: "post-live",
            evidenceLinks: [],
            detailLines: ["report detail"],
          },
          {
            id: "wire-shipped",
            kind: "shipped",
            source: "glass",
            title: "Patch shipped",
            summary: "Merged locally",
            occurredAt: now - 120,
            agent: "shipper",
            sessionId: null,
            sessionTitle: null,
            postId: null,
            evidenceLinks: [],
            detailLines: [],
          },
        ],
        dead: { agentCount: 0, sessionCount: 0, sessions: [] },
        notices: [],
        landmark: { status: "ok", message: null },
      }),
    });
  });

  await page.goto("/");

  const rows = page.locator(".glass-now-row");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toContainText("z-blocked");
  await expect(rows.nth(0)).toContainText("blocked 42m — waiting for ingest key");
  await expect(rows.nth(1)).toContainText("alpha-live");
  await expect(rows.nth(1)).toContainText("live · shaping the viewer");
  await expect(rows.nth(2)).toContainText("m-quiet");
  await expect(rows.nth(2)).toContainText("quiet · 1d");

  await page.getByRole("button", { name: "Name" }).click();
  await expect(rows.nth(0)).toContainText("alpha-live");
  await expect(rows.nth(1)).toContainText("m-quiet");
  await expect(rows.nth(2)).toContainText("z-blocked");

  await expect(page.locator(".glass-wire-legend")).toContainText("blocked");
  await expect(page.locator(".glass-wire-pinned")).toContainText(
    "NEEDS ATTENTION · 2",
  );
  await expect(page.locator(".glass-wire-pinned")).toContainText(
    "Canary key blocks deploy",
  );
  await expect(page.locator(".glass-wire-pinned")).toContainText(
    "Choose report window",
  );
  await expect(page.locator(".glass-wire-tape-row")).toHaveCount(2);
  await expect(page.locator(".glass-wire-tape-row").first()).toContainText(
    "Report generated",
  );

  await page.locator(".glass-wire-tape-row", { hasText: "Report generated" }).click();
  await expect(page.locator("#feed-dialog")).toBeVisible();
  await expect(page.locator("#feed-dialog")).toContainText("Report generated");
  await page.locator("[data-feed-close]").click();
});

test("reports sentence builder renders and caches in place", async ({
  page,
}) => {
  await setSystemMode(page);
  await page.goto("/reports");
  await expectSharedRail(page, "Reports");

  await expect(page.getByText("REPORT QUERY")).toBeVisible();
  await expect(page.getByText("Show me")).toBeVisible();
  await expect(page.locator("#reports-scope")).toHaveValue("fleet");
  await expect(page.locator("#reports-window")).toHaveValue("past-24h");
  await page.getByRole("button", { name: "Run" }).click();
  await expect(page).toHaveURL(/\/reports$/);
  await expectSharedRail(page, "Reports");
  const report = page.locator("#reports-result .reports-doc");
  await expect(report).toContainText("Activity digest - fleet");
  await expect(report).toContainText("Rendered e2e seed");
  await expect(report).toContainText("powder-unattributed");
  await expect(report.locator(".glass-rep-hero")).toBeVisible();
  await expect(report.locator(".glass-rep-pipeline")).toBeVisible();
  await expect(report.locator(".glass-rep-callouts")).toBeVisible();
  const docClasses = await report.locator(".reports-doc-body").evaluate((el) =>
    Array.from(el.children).map((child) => (child as HTMLElement).className),
  );
  expect(docClasses[0]).toContain("glass-rep-hero");
  expect(docClasses[1]).toContain("glass-rep-prose");
  expect(docClasses[2]).toContain("glass-rep-pipeline");
  expect(docClasses[3]).toContain("glass-rep-prose");
  expect(docClasses[4]).toMatch(/glass-rep-(evidence|exhibit)/);
  expect(docClasses[5]).toContain("glass-rep-prose");
  expect(docClasses[6]).toContain("glass-rep-callouts");
  expect(docClasses.at(-1)).toContain("glass-rep-caption");
  await expect(page.locator("#reports-status")).toContainText("generated");

  await page.getByRole("button", { name: "Run" }).click();
  await expect(page.locator("#reports-status")).toContainText("cached · generated");
  await expect(page.getByRole("button", { name: "regenerate" })).toBeVisible();
});

test("agent page renders AGENT-8 tabs and scoped reports", async ({ page }) => {
  await setSystemMode(page);
  await page.goto("/agent/e2e-agent");
  await expectSharedRail(page, "Now");

  await expect(page.locator("#agent-page")).toBeVisible();
  await expect(page.locator(".glass-agent-name")).toHaveText("e2e-agent");
  await expect(page.locator("#agent-state-head")).toContainText("powder glass-932");
  await expect(page.locator("#agent-tab-wire")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator("#agent-panel-wire")).toBeVisible();
  await expect(page.locator("#agent-panel-report")).toBeHidden();
  await expect(page.locator("#agent-wire-feed .glass-wire-tape-row")).toHaveCount(
    1,
  );
  await expect(page.locator("#agent-wire-feed")).toContainText(
    "Rendered e2e seed",
  );
  await expect(page.locator("#posts")).toBeHidden();

  await page.getByRole("tab", { name: "Report" }).click();
  await expect(page).toHaveURL(/#report$/);
  await expect(page.locator("#agent-tab-report")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator("#agent-panel-wire")).toBeHidden();
  await expect(page.locator("#agent-panel-report")).toBeVisible();
  await expect(page.locator("#agent-report-scope")).toHaveText(
    "agent e2e-agent",
  );

  await page.locator("#agent-report-run").click();
  const scopedReport = page.locator("#agent-report-result .reports-doc");
  await expect(scopedReport).toContainText("Activity digest - agent e2e-agent");
  await expect(scopedReport).toContainText("Rendered e2e seed");
  await expect(scopedReport).not.toContainText("powder-unattributed");
  await expect(scopedReport.locator(".glass-rep-hero")).toBeVisible();

  await page.goto("/agent/e2e-agent#report");
  await expect(page.locator("#agent-tab-report")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator("#agent-panel-report")).toBeVisible();

  await page.goto("/agent/e2e-agent#wire");
  await expect(page.locator("#agent-tab-wire")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.locator("#agent-panel-wire")).toBeVisible();
});

test("operator can walk every rail place from Now", async ({ page }) => {
  await setSystemMode(page);
  await page.goto("/");
  await expectSharedRail(page, "Now");

  await page.getByRole("link", { name: "Needs you · 2" }).click();
  await expect(page).toHaveURL(/\/needs-you$/);
  await expectSharedRail(page, "Needs you · 2");

  await page.goto("/");
  await page.getByRole("link", { name: "Reports" }).click();
  await expect(page).toHaveURL(/\/reports$/);
  await expectSharedRail(page, "Reports");

  await page.goto("/");
  await expectSharedRail(page, "Now");
});

test("clips human route redirects to Now while the API stays available", async ({
  page,
  request,
}) => {
  const redirect = await request.get("/clips", { maxRedirects: 0 });
  expect(redirect.status()).toBe(301);
  expect(redirect.headers()["location"]).toBe("/");

  const api = await request.get("/api/clips?limit=10");
  expect(api.ok()).toBe(true);
  const body = await api.json();
  expect(Array.isArray(body.clips)).toBe(true);

  await page.goto("/clips");
  await expect(page).toHaveURL(/\/$/);
  await expectSharedRail(page, "Now");
});

test("report handles still open without a reports library link", async ({
  page,
  request,
}) => {
  const seed = await request.post("/api/reports", {
    data: {
      kind: "activity-digest",
      scope: { type: "fleet" },
      window: "past-24h",
      requestedBy: "e2e-setup",
    },
  });
  expect(seed.ok()).toBe(true);
  const seeded = await seed.json();

  await setSystemMode(page);
  await page.goto("/reports");
  await expectSharedRail(page, "Reports");
  await expect(page.locator(`a[href="${seeded.url}"]`)).toHaveCount(0);

  await page.goto(seeded.url);
  await expect(page).toHaveURL(new RegExp(`${seeded.url}$`));
  await expectSharedRail(page, "Reports");
  await expect(page.getByText("Activity digest - fleet").first()).toBeVisible();
  await expect(page.getByText("Rendered e2e seed")).toBeVisible();
});

test("shared shell rail renders on every human HTML route", async ({ page }) => {
  await setSystemMode(page);

  const routes: Array<[string, string | null]> = [
    ["/", "Now"],
    ["/agent/e2e-agent", "Now"],
    ["/reports", "Reports"],
    ["/needs-you", "Needs you · 2"],
    ["/review/sample", null],
  ];

  for (const [path, active] of routes) {
    await page.goto(path);
    await expectSharedRail(page, active);
  }

  await page.goto("/");
  const detailHref = await page
    .locator(".glass-wire-tape-row", { hasText: "Rendered e2e seed" })
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

test("shared rail becomes a burger sheet at 390px", async ({ page }) => {
  await setSystemMode(page);
  await page.setViewportSize({ width: 390, height: 760 });
  await page.goto("/needs-you");

  await expect(page.locator(".glass-topbar")).toBeVisible();
  await expect(page.locator(".glass-topbar .ae-logo")).toContainText("GLASS");
  await expect(page.locator(".glass-top-needs")).toContainText("2");
  await expect(page.locator(".glass-shell")).toHaveAttribute(
    "data-nav-open",
    "false",
  );

  const closedRailBox = await page.locator(".glass-rail").boundingBox();
  expect(closedRailBox).toBeTruthy();
  expect(closedRailBox!.x).toBeLessThan(0);

  await page.getByRole("button", { name: "Open navigation" }).click();
  await expect(page.locator(".glass-shell")).toHaveAttribute(
    "data-nav-open",
    "true",
  );
  await expect
    .poll(async () => {
      const box = await page.locator(".glass-rail").boundingBox();
      return box ? Math.round(box.x) : -999;
    })
    .toBe(0);
  const openRailBox = await page.locator(".glass-rail").boundingBox();
  expect(openRailBox!.height).toBeGreaterThan(700);
  await expect(page.locator(".glass-rail")).toContainText("PLACES");
  await expect(page.locator(".glass-rail")).toContainText("Needs you");

  await page.locator(".glass-nav-scrim").click({ position: { x: 360, y: 40 } });
  await expect(page.locator(".glass-shell")).toHaveAttribute(
    "data-nav-open",
    "false",
  );
  await expect
    .poll(async () => {
      const box = await page.locator(".glass-rail").boundingBox();
      return box ? Math.round(box.x) : 0;
    })
    .toBeLessThan(0);

  const scroll = await page.evaluate(() => ({
    body: document.body.scrollWidth,
    doc: document.documentElement.scrollWidth,
    win: window.innerWidth,
  }));
  expect(scroll.doc).toBeLessThanOrEqual(scroll.win);
  expect(scroll.body).toBeLessThanOrEqual(scroll.win);
});
