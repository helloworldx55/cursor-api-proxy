import http from "node:http";

const host = process.env.CURSOR_BRIDGE_HOST || "127.0.0.1";
const port = Number(process.env.CURSOR_BRIDGE_PORT || "8765");

const server = http.createServer((req, res) => {
  if (req.url === "/health" || req.url === "/healthz") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ status: "ok" }));
    return;
  }
  res.writeHead(404);
  res.end();
});

server.listen(port, host);
