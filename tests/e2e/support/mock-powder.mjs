import http from "node:http";

const port = Number(process.env.GLASS_E2E_POWDER_PORT || 19042);
const now = Math.floor(Date.now() / 1000);

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
      run_id: "run-now-rich",
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
      run_id: "run-now-quiet",
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
    updated_at: now - 86_400,
  },
];

function seedAwaiting() {
  const now = Math.floor(Date.now() / 1000);
  return [
    {
      card: {
        id: "glass-931",
        title: "Redesign 1/6 - the shell",
        repo: "glass",
        priority: "p1",
      },
      question: {
        payload: "DECIDE: keep the rail active on viewer drill-downs?",
        created_at: now - 120,
      },
      run: {
        id: "run-shell",
        agent: "glass-931-codex",
        created_at: now - 240,
      },
    },
    {
      card: {
        id: "glass-933",
        title: "Reports home",
        repo: "glass",
        priority: "p2",
      },
      question: {
        payload: "ACT: confirm reports URL flip",
        created_at: now - 600,
      },
      run: {
        id: "run-reports",
        agent: "glass-933-codex",
        created_at: now - 900,
      },
    },
  ];
}

let awaiting = seedAwaiting();
let answered = [];

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? "/", `http://${req.headers.host}`);
  if (url.pathname === "/health") {
    res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
    res.end("ok");
    return;
  }
  if (req.method === "POST" && url.pathname === "/__reset") {
    awaiting = seedAwaiting();
    answered = [];
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
  if (url.pathname === "/api/v1/runs/awaiting-input") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ awaiting, answered }));
    return;
  }
  const answerMatch = url.pathname.match(/^\/api\/v1\/runs\/([^/]+)\/answer$/);
  if (req.method === "POST" && answerMatch) {
    let body = "";
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      const runId = answerMatch[1];
      const index = awaiting.findIndex((item) => item.run.id === runId);
      if (index === -1) {
        res.writeHead(404, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "run not awaiting input" }));
        return;
      }
      let parsed = {};
      try {
        parsed = JSON.parse(body || "{}");
      } catch (e) {}
      const [item] = awaiting.splice(index, 1);
      answered.push({
        ...item,
        answer: parsed.answer ?? "",
        answered_at: Math.floor(Date.now() / 1000),
      });
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ ok: true, run: { ...item.run, state: "active" } }));
    });
    return;
  }
  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
});

server.listen(port, "127.0.0.1");
