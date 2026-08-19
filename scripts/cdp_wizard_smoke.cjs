// 向导全流程冒烟：开始配置 → 填 Key → 验证并保存 → 以后再说 → 开始使用。
// 用法：
//   node scripts/cdp_wizard_smoke.cjs <port> <apiKey> <outPng>
// 重点断言每一步的 DOM 文本；任何一步不符都会打印当前文本并 exit 1。
async function main() {
  const port = Number(process.argv[2]);
  const apiKey = process.argv[3];
  const outPng = process.argv[4];
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
  const ev = (expression) => cmd("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })
    .then((r) => {
      if (r.exceptionDetails) throw new Error("eval: " + JSON.stringify(r.exceptionDetails));
      return r.result.value;
    });
  const text = () => ev("document.body ? document.body.innerText : ''");
  const shot = async () => {
    if (!outPng) return;
    const s = await cmd("Page.captureScreenshot", { format: "png" });
    const fs = await import("node:fs");
    fs.writeFileSync(outPng, Buffer.from(s.data, "base64"));
  };
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));

  const step = async (name, expect, fn, timeoutMs = 20000) => {
    const t0 = Date.now();
    while (Date.now() - t0 < timeoutMs) {
      const cur = await text();
      if (cur.includes(expect)) {
        console.log(`PASS ${name}（${(Date.now() - t0) / 1000}s）`);
        return;
      }
      try { await fn(); } catch { /* 元素未就绪则重试 */ }
      await wait(600);
    }
    console.error(`FAIL ${name}: 期待「${expect}」，当前文本：`);
    console.error(await text());
    process.exit(1);
  };

  // 1. 欢迎页 → 点击「开始配置」
  await step("欢迎页", "你好，我是璃", async () => {
    await ev(`(() => { const b = [...document.querySelectorAll('button')].find(x => x.textContent.includes('开始配置')); if (b) b.click(); return !!b; })()`);
  });
  // 2. API Key 页：填入 key（React 受控组件需原生 setter）
  await step("API Key 页", "连接智能大脑", async () => {
    await ev(`(() => {
      const inp = document.querySelector('input[type="password"]');
      if (!inp) return false;
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
      setter.call(inp, ${JSON.stringify(apiKey)});
      inp.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    })()`);
  });
  // 3. 点击「验证并保存」→ 真实 LLM 调用，期待出现「验证通过」并进入模型页
  await step("验证通过 → 模型页", "记忆模型", async () => {
    await ev(`(() => { const b = [...document.querySelectorAll('button')].find(x => x.textContent.includes('验证并保存')); if (b) b.click(); return !!b; })()`);
  });
  await shot();
  // 4. 点击「以后再说」→ 完成页
  await step("完成页", "准备好了", async () => {
    await ev(`(() => { const b = [...document.querySelectorAll('button')].find(x => x.textContent.includes('以后再说')); if (b) b.click(); return !!b; })()`);
  });
  // 5. 点击「开始和璃相处」→ 向导关闭，进入正常桌宠模式
  const t5 = Date.now();
  let closed = false;
  while (Date.now() - t5 < 8000) {
    await ev(`(() => { const b = [...document.querySelectorAll('button')].find(x => x.textContent.includes('开始和璃相处')); if (b) b.click(); return !!b; })()`);
    await wait(600);
    const cur = await text();
    if (!cur.includes("开始和璃相处") && !cur.includes("你好，我是璃") && !cur.includes("准备好了")) {
      closed = true;
      break;
    }
  }
  if (!closed) {
    console.error("FAIL 向导未关闭，当前文本：");
    console.error(await text());
    process.exit(1);
  }
  console.log("PASS 向导已关闭，进入桌宠模式");
  ws.close();
}
main().catch((e) => { console.error("SMOKE ERROR:", e.message); process.exit(1); });