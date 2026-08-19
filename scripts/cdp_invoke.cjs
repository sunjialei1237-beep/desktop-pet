// 对运行中的桌宠实例调用 Tauri command（冒烟用）：Usage: node cdp_invoke.cjs <port> <command> [jsonArgs]
async function main() {
  const port = Number(process.argv[2]);
  const command = process.argv[3];
  const argsRaw = process.argv[4];
  const args = argsRaw ? JSON.parse(argsRaw) : {};
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  const ev = (expression) => new Promise((res) => {
    const i = Math.floor(Math.random() * 1e9);
    const onmsg = (e) => {
      const m = JSON.parse(e.data);
      if (m.id !== i) return;
      ws.removeEventListener("message", onmsg);
      res(m.result);
    };
    ws.addEventListener("message", onmsg);
    ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression, returnByValue: true, awaitPromise: true } }));
  });
  const expr = `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)}).then(v => JSON.stringify(v), e => "ERROR: " + JSON.stringify(e))`;
  const r = await ev(expr);
  console.log(r.result?.value ?? JSON.stringify(r));
  ws.close();
}
main().catch((e) => { console.error("ERR", e.message); process.exit(1); });