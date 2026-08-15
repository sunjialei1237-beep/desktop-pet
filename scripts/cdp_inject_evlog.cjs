// Inject window-level mouse event logging into the pet webview via CDP.
// Usage: node scripts/cdp_inject_evlog.cjs <port> [action]
//   action "inject" (default): install listeners, return ok
//   action "read": return the collected __evLog
const port = Number(process.argv[2] || 9223);
const action = process.argv[3] || "inject";
async function main() {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const ev = (expression) => new Promise((res, rej) => {
    const i = ++id;
    const h = (m) => {
      const d = JSON.parse(m.data);
      if (d.id !== i) return;
      ws.removeEventListener("message", h);
      if (d.error || d.result.exceptionDetails) rej(new Error(JSON.stringify(d.error || d.result.exceptionDetails)));
      else res(d.result.result.value);
    };
    ws.addEventListener("message", h);
    ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
  if (action === "inject") {
    const expr = `(() => {
      window.__evLog = [];
      const types = ['mousedown','mousemove','mouseup','pointerdown','pointermove','pointerup'];
      for (const t of types) {
        window.addEventListener(t, (e) => {
          window.__evLog.push([t, Math.round(e.clientX), Math.round(e.clientY), e.button]);
          if (window.__evLog.length > 200) window.__evLog.shift();
        });
      }
      return 'injected ' + types.length;
    })()`;
    console.log(await ev(expr));
  } else if (action === "read") {
    console.log(JSON.stringify(await ev(`window.__evLog || []`)));
  }
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
