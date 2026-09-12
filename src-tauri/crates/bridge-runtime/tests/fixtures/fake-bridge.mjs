import http from "node:http";

const host = process.env.CURSOR_BRIDGE_HOST || "127.0.0.1";
const port = Number(process.env.CURSOR_BRIDGE_PORT || "8765");
const bridgeToken = process.env.CURSOR_BRIDGE_API_KEY || "";

if (bridgeToken) {
  console.log(`leaked Bridge Token ${bridgeToken}`);
}

const server = http.createServer((req, res) => {
  if (req.url === "/health" || req.url === "/healthz") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ status: "ok" }));
    return;
  }

  const path = req.url?.split("?")[0] ?? "";
  if (path.startsWith("/v1")) {
    const auth = req.headers.authorization ?? "";
    const expected = `Bearer ${bridgeToken}`;
    if (!bridgeToken || auth !== expected) {
      res.writeHead(401, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "unauthorized" }));
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ object: "list", data: [] }));
    return;
  }

  res.writeHead(404);
  res.end();
});

server.listen(port, host);
