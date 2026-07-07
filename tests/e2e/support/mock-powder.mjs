import http from "node:http";

const port = Number(process.env.GLASS_E2E_POWDER_PORT || 19042);

const cards = [
  {
    id: "glass-905",
    title: "Application floor: add coverage and rendered e2e gates",
    status: "ready",
    priority: "p2",
    blocked_by: [],
    updated_at: Math.floor(Date.now() / 1000),
  },
  {
    id: "glass-901",
    title: "Native service MVP",
    status: "done",
    priority: "p1",
    blocked_by: [],
    updated_at: Math.floor(Date.now() / 1000) - 86_400,
  },
];

const server = http.createServer((req, res) => {
  const url = new URL(req.url ?? "/", `http://${req.headers.host}`);
  if (url.pathname === "/health") {
    res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
    res.end("ok");
    return;
  }
  if (url.pathname === "/api/v1/cards") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ cards }));
    return;
  }
  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
});

server.listen(port, "127.0.0.1");
