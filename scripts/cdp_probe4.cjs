// CDP probe #4: poll window.__dragDiag every 400ms for ~8s while the external
// drag test runs. Prints the gate values so we can see the handshake live.
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
    ws.send(JSON.stringify({ id: i, method: "Runtime.evaluate", params: { expression, returnByValue: true, awaitPromise: true } }));
  });
  const has = await ev(`typeof window.__dragDiag`);
  if (has !== "function") throw new Error("__dragDiag not exposed (HMR not applied?): " + has);
  for (let i = 0; i < 20; i++) {
    const v = await ev(`JSON.stringify(window.__dragDiag())`);
    console.log(`${i * 400}ms ${v}`);
    await new Promise(r => setTimeout(r, 400));
  }
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
