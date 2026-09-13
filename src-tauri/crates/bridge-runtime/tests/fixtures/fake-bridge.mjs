import http from "node:http";

const host = process.env.CURSOR_BRIDGE_HOST || "127.0.0.1";
const port = Number(process.env.CURSOR_BRIDGE_PORT || "8765");
const bridgeToken = process.env.CURSOR_BRIDGE_API_KEY || "";
const cursorApiKey = process.env.CURSOR_API_KEY || "";

if (bridgeToken) {
  console.log(`leaked Bridge Token ${bridgeToken}`);
}
if (cursorApiKey) {
  console.log(`leaked Cursor API Key ${cursorApiKey}`);
}

const extraLogBytes = Number(process.argv[2] || "0");
if (Number.isFinite(extraLogBytes) && extraLogBytes > 0) {
  console.log(`${"x".repeat(extraLogBytes)}\nLOG_END_MARKER`);
}

const server = http.createServer((req, res) => {
  if (req.url === "/health" || req.url === "/healthz") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        status: "ok",
        has_cursor_api_key: Boolean(cursorApiKey),
        chat_only_workspace: process.env.CURSOR_BRIDGE_CHAT_ONLY_WORKSPACE ?? "",
        prompt_via_stdin: process.env.CURSOR_BRIDGE_PROMPT_VIA_STDIN ?? "",
      }),
    );
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
    const reply = () => {
      if (req.method === "POST" && path === "/v1/chat/completions") {
        res.writeHead(200, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            id: "chatcmpl-fake",
            object: "chat.completion",
            choices: [{ index: 0, message: { role: "assistant", content: "ok" }, finish_reason: "stop" }],
          }),
        );
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ object: "list", data: [] }));
    };
    if (req.method === "POST") {
      req.on("data", () => {});
      req.on("end", reply);
      return;
    }
    reply();
    return;
  }

  res.writeHead(404);
  res.end();
});

server.listen(port, host);
