// Dump geometry + diag from the pet webview via CDP.
// Usage: node scripts/cdp_geom_dump.cjs [port]
const port = Number(process.argv[2] || 9223);
async function main() {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  const ev = (expression) => new Promise((res, rej) => {
    const h = (m) => {
      const d = JSON.parse(m.data);
      if (d.id !== 1) return;
      ws.removeEventListener("message", h);
      if (d.error || d.result.exceptionDetails) rej(new Error(JSON.stringify(d.error || d.result.exceptionDetails)));
      else res(d.result.result.value);
    };
    ws.addEventListener("message", h);
    ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
  console.log("diag:", await ev(`JSON.stringify(window.__dragDiag ? window.__dragDiag() : null)`));
  console.log("canvasRect:", await ev(`(() => { const c = document.querySelector('canvas'); if (!c) return 'no canvas'; const r = c.getBoundingClientRect(); return JSON.stringify({ left: r.left, top: r.top, width: r.width, height: r.height }); })()`));
  console.log("wrapperRect:", await ev(`(() => { const el = document.querySelector('.pet-char-wrapper'); if (!el) return 'no wrapper'; const r = el.getBoundingClientRect(); return JSON.stringify({ left: r.left, top: r.top, width: r.width, height: r.height }); })()`));
  ws.close();
}
main().catch((e) => { console.error("DUMP_FAIL", e.message); process.exit(1); });
