// 访谈气泡时间线观察（CDP）：轮询气泡状态，只在变化时打印。
// 用法: node scripts/cdp_timeline.cjs <port> <durationMs>
async function main() {
  const port = Number(process.argv[2] || 9222);
  const dur = Number(process.argv[3] || 150000);
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0;
  const pending = new Map();
  ws.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) {
      const { res, rej } = pending.get(m.id);
      pending.delete(m.id);
      if (m.error || (m.result && m.result.exceptionDetails))
        rej(new Error(JSON.stringify(m.error || m.result.exceptionDetails)));
      else res(m.result.result.value);
    }
  });
  const ev = (expression) => new Promise((res, rej) => {
    const i = ++id;
    pending.set(i, { res, rej });
    ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });

  const read = () => ev(`(() => {
    const b = document.querySelector('.pet-bubble');
    return b ? (b.textContent || '').slice(0, 30) : 'NO_BUBBLE';
  })()`);

  const t0 = Date.now();
  let last = undefined;
  while (Date.now() - t0 < dur) {
    const s = await read();
    if (s !== last) { console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s] ${s}`); last = s; }
    await new Promise((r) => setTimeout(r, 300));
  }
  console.log(`[end] final=${await read()}`);
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
