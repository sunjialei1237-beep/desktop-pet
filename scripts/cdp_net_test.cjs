// 访谈兜底网验证（CDP）：
//   1) 读取当前气泡/输入框状态
//   2) 用 dblclick（应用自己的隐藏路径：setBubbleVisible(false)+clearTimeout）把气泡藏掉
//   3) 轮询 3s，观察气泡是否在 ~400ms 后被兜底网重新亮出
// 用法: node scripts/cdp_net_test.cjs [port]
async function main() {
  const port = Number(process.argv[2] || 9222);
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

  const readState = () => ev(`(() => {
    const b = document.querySelector('.pet-bubble');
    const input = document.querySelector('.input-bubble input');
    return JSON.stringify({
      bubbleExists: !!b,
      bubbleText: b ? (b.textContent || '').slice(0, 40) : null,
      bubbleVisibleClass: b ? b.className : null,
      inputExists: !!input,
      t: Date.now(),
    });
  })()`);

  console.log("[初始]", await readState());

  // 触发应用自己的隐藏路径：包装 div 上派发 dblclick（onDoubleClick → setBubbleVisible(false)）
  const dispatched = await ev(`(() => {
    const w = document.querySelector('.pet-char-wrapper');
    if (!w) return 'NO_WRAPPER';
    w.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    return 'DBLCLICK_SENT';
  })()`);
  console.log("[动作]", dispatched);

  const t0 = Date.now();
  let last = null;
  while (Date.now() - t0 < 3000) {
    const s = await readState();
    if (s !== last) { console.log(`[${Date.now() - t0}ms]`, s); last = s; }
    await new Promise((r) => setTimeout(r, 100));
  }
  const final = JSON.parse(await readState());
  console.log(final.bubbleExists ? `\n✓ 兜底网生效：气泡已重新显示「${final.bubbleText}」` : "\n✗ 兜底网未生效：气泡没有回来");
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
