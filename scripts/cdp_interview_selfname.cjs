// 引导访谈四题自动作答（CDP）：第 4 题回「你来想」，验证她真的调 LLM 给自己起名。
// 用法: node scripts/cdp_interview_selfname.cjs [port]
const ANSWERS = ["你可以叫我主人", "温柔活泼", "我们是好伙伴", "你来想"];

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

  const bubbleText = () => ev(`(() => {
    const b = document.querySelector('.pet-bubble');
    return b ? (b.textContent || '').slice(0, 60) : 'NO_BUBBLE';
  })()`);
  const answer = (text) => ev(`(() => {
    const input = document.querySelector('.input-bubble input');
    if (!input) return 'NO_INPUT';
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
    setter.call(input, ${JSON.stringify(text)});
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    return 'SENT';
  })()`);

  // 等访谈首题出现（最多 30s）
  for (let i = 0; i < 60; i++) {
    if ((await bubbleText()).includes("初次见面")) break;
    await new Promise((r) => setTimeout(r, 500));
  }
  console.log("[Q1]", await bubbleText());

  for (let q = 0; q < ANSWERS.length; q++) {
    console.log(`[答${q + 1}]`, ANSWERS[q], "->", await answer(ANSWERS[q]));
    await new Promise((r) => setTimeout(r, 1200));
    console.log(`[气泡]`, await bubbleText());
  }

  // 第 4 题后：思考球 → LLM 起名 → 收尾气泡（最长 25s）
  const t0 = Date.now();
  let last = "";
  while (Date.now() - t0 < 25000) {
    const s = await bubbleText();
    if (s !== last) { console.log(`[${((Date.now() - t0) / 1000).toFixed(1)}s]`, s); last = s; }
    if (s.includes("「") || s.includes("认识你真高兴")) break;
    await new Promise((r) => setTimeout(r, 300));
  }
  ws.close();
}
main().catch((e) => { console.error("PROBE_FAIL", e.message); process.exit(1); });
