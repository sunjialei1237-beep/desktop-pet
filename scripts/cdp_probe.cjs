// CDP probe: read the pet app's own click-through geometry so the drag-test
// grab point lands on the model (not a click-through region).
// Usage: node scripts/cdp_probe.cjs
async function main() {
  const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const ev = (expression) => new Promise((res, rej) => {
    const i = ++id;
    const onmsg = (e) => {
      const m = JSON.parse(e.data);
      if (m.id !== i) return;
      ws.removeEventListener("message", onmsg);
      if (m.error) rej(new Error(JSON.stringify(m.error)));
      else if (m.result.exceptionDetails) rej(new Error(JSON.stringify(m.result.exceptionDetails)));
      else res(m.result.result.value);
    };
    ws.addEventListener("message", onmsg);
    ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
  const out = await ev(`(() => {
    const r = document.querySelector('canvas').getBoundingClientRect();
    const ct = window.__ctDiag ? window.__ctDiag() : null;
    const gz = window.__gazeDiag || null;
    return JSON.stringify({
      canvas: { left: r.left, top: r.top, w: r.width, h: r.height },
      ctDiag: ct,
      gaze: gz ? { hx: gz.hx, hy: gz.hy, cx: gz.cx, cy: gz.cy } : null,
      inner: { w: window.innerWidth, h: window.innerHeight },
      dpr: window.devicePixelRatio,
    });
  })()`);
  console.log(out);
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
