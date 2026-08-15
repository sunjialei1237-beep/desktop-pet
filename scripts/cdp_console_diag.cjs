// Capture consoleAPICalled (warn/error) + poll window.__dragDiag on a CDP port.
// Usage: node scripts/cdp_console_diag.cjs <port> <durationMs> [diagIntervalMs]
async function main() {
  const port = Number(process.argv[2] || 9222);
  const dur = Number(process.argv[3] || 12000), int = Number(process.argv[4] || 100);
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target");
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
  const ev = (expression) => new Promise((res, rej) => {
    const i = ++id;
    const onmsg = (e) => {
      const m = JSON.parse(e.data);
      if (m.id !== i) return;
      ws.removeEventListener("message", onmsg);
      if (m.error || m.result.exceptionDetails) rej(new Error(JSON.stringify(m.error || m.result.exceptionDetails)));
      else res(m.result.result.value);
    };
    ws.addEventListener("message", onmsg);
    ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
  ws.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.method === "Runtime.consoleAPICalled" && m.params) {
      const args = (m.params.args || []).map((a) => a.value ?? a.description ?? "").join(" ");
      console.log(`[console:${m.params.type}] ${args}`);
    }
  });
  await cmd("Runtime.enable");
  const t0 = Date.now();
  while (Date.now() - t0 < dur) {
    const v = await ev(`typeof window.__dragDiag === 'function' ? JSON.stringify(window.__dragDiag()) : 'NO_DRAGDIAG'`);
    console.log(`${Date.now() - t0}ms ${v}`);
    await new Promise((r) => setTimeout(r, int));
  }
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
