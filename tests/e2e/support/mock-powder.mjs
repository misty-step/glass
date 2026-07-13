import http from "node:http";

const port = Number(process.env.GLASS_E2E_POWDER_PORT || 19042);
const now = Math.floor(Date.now() / 1000);
const lastWeek = now - 7 * 86_400;

const cards = [
  {
    id: "glass-932",
    title: "Now fleet wall from Powder claims and live sessions",
    status: "running",
    priority: "p1",
    repo: "glass",
    blocked_by: [],
    claim: {
      agent: "e2e-agent",
      id: "claim-now-rich",
      runtime_ref: "run-now-rich",
      acquired_at: now - 300,
      expires_at: now + 3_600,
    },
    updated_at: now - 30,
  },
  {
    id: "glass-quiet",
    title: "Claimed lane with no Glass posts",
    status: "claimed",
    priority: "p2",
    repo: "glass",
    blocked_by: [],
    claim: {
      agent: "quiet-agent",
      id: "claim-now-quiet",
      runtime_ref: "run-now-quiet",
      acquired_at: now - 1_320,
      expires_at: now + 3_600,
    },
    updated_at: now - 1_320,
  },
  {
    id: "glass-905",
    title: "Application floor: add coverage and rendered e2e gates",
    status: "ready",
    priority: "p2",
    blocked_by: [],
    updated_at: now,
  },
  {
    id: "glass-901",
    title: "Native service MVP",
    status: "done",
    priority: "p1",
    blocked_by: [],
    updated_at: lastWeek,
  },
  {
    id: "powder-unattributed",
    title: "Unattributed fleet completion",
    status: "done",
    priority: "p2",
    repo: "powder",
    blocked_by: [],
    updated_at: now - 100,
  },
];

function seedAwaiting() {
  const now = Math.floor(Date.now() / 1000);
  return [
    {
      id: "ask-shell",
      run_id: "run-shell",
      task: "glass-931-codex",
      kind: "decision",
      question: "DECIDE: keep the rail active on viewer drill-downs?",
      context: "Redesign 1/6 - the shell",
      blocking: true,
      state: "parked",
      created_at: new Date((now - 120) * 1000).toISOString(),
      answer: null,
    },
    {
      id: "ask-reports",
      run_id: "run-reports",
      task: "glass-933-codex",
      kind: "act",
      question: "ACT: confirm reports URL flip",
      context: "Reports home",
      blocking: false,
      state: "open",
      created_at: new Date((now - 600) * 1000).toISOString(),
      answer: null,
    },
  ];
}

let awaiting = seedAwaiting();

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? "/", `http://${req.headers.host}`);
  if (url.pathname === "/health") {
    res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
    res.end("ok");
    return;
  }
  if (req.method === "POST" && url.pathname === "/__reset") {
    awaiting = seedAwaiting();
    res.writeHead(204);
    res.end();
    return;
  }
  if (url.pathname === "/api/v1/cards") {
    const status = url.searchParams.get("status");
    const filtered = status
      ? cards.filter((card) => card.status === status)
      : cards;
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ cards: filtered }));
    return;
  }
  if (url.pathname === "/api/asks") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(awaiting));
    return;
  }
  const answerMatch = url.pathname.match(/^\/api\/asks\/([^/]+)\/answer$/);
  if (req.method === "POST" && answerMatch) {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      const askId = answerMatch[1];
      const index = awaiting.findIndex((item) => item.id === askId);
      if (index === -1) {
        res.writeHead(404, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "ask not awaiting input" }));
        return;
      }
      let parsed = {};
      try {
        parsed = JSON.parse(body || "{}");
      } catch (e) {}
      const [item] = awaiting.splice(index, 1);
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ ask: { ...item, answer: parsed.answer ?? "" }, resumed_run_id: "run-resume" }));
    });
    return;
  }
  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
});

server.listen(port, "127.0.0.1");
