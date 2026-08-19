// CDP DOM 快照 + 截图工具（验证安装版首次启动向导渲染）。
// 用法：node scripts/cdp_snapshot.cjs <port> [outPng]
async function main() {
  const port = Number(process.argv[2] || 9226);
  const outPng = process.argv[3];
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target on port " + port);
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const cmd = (method, params = {}) => new Promise((res, rej) => {
    const i = ++id;
    const onmsg = (e) => {
      const m = JSON.parse(e.data);
      if (m.id !== i) return;
      ws.removeEventListener("message", onmsg);
      if (m.error) rej(new Error(JSON.stringify(m.error)));
      else res(m.result);
    };
    ws.addEventListener("message", onmsg);
    ws.send(JSON.stringify({ id: i, method, params }));
  });
  const text = await cmd("Runtime.evaluate", {
    expression: `document.body ? document.body.innerText.slice(0, 2000) : "(no body)"`,
    returnByValue: true,
  });
  console.log("=== DOM TEXT ===");
  console.log(text.result.value);
  if (outPng) {
    const shot = await cmd("Page.captureScreenshot", { format: "png" });
    const fs = await import("node:fs");
    fs.writeFileSync(outPng, Buffer.from(shot.data, "base64"));
    console.log("=== SCREENSHOT SAVED:", outPng);
  }
  ws.close();
}
main().catch((e) => { console.error("CDP ERROR:", e.message); process.exit(1); });