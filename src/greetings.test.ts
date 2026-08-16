import { describe, it, expect, beforeEach } from "vitest";
import { pickGreeting, resetGreetingMemory } from "./greetings";

// Known pool contents (mirror of src/greetings.ts) so tests can assert bucket
// membership precisely.
const BRIEF = ["咦，刚刚那下卡住了？", "我眨了下眼，你就回来了。", "嗯，在的。", "重启好啦，继续陪你。", "刚缓过来，没事了。"];
const DAY = ["你回来啦。", "嗯，等你半天了。", "刚还在想你去哪了。", "回来啦，喝水了吗？", "嗯，回来了就好。", "我先把刚才的念头收好了。"];
const OVERNIGHT = ["我睡了好久……你回来啦。", "睡了一觉，你一直没来。", "醒来啦，你呢？", "刚才做了个好长的梦。", "我睡饱了，你呢？"];
const MORNING = ["早呀，新的一天。", "这么早就来啦。", "早上好，吃早饭了吗？"];
const NIGHT = ["这么晚才来呀。", "夜里也想起我了？", "都这个点了，陪我一小会儿就好。"];
const DEEPNIGHT = ["……你怎么还没睡。", "这个点来，是睡不着吗？"];

describe("pickGreeting", () => {
  beforeEach(() => resetGreetingMemory());

  it("buckets by away duration: <1h brief, 1-8h day, >=8h overnight", () => {
    const brief = pickGreeting({ awayHours: 0.4, hourOfDay: 14 }, () => 0);
    const day = pickGreeting({ awayHours: 3, hourOfDay: 14 }, () => 0);
    const overnight = pickGreeting({ awayHours: 14, hourOfDay: 14 }, () => 0);
    expect(BRIEF).toContain(brief);
    expect(DAY).toContain(day);
    expect(OVERNIGHT).toContain(overnight);
  });

  it("merges time-of-day extras into the pool (morning / night / deep night)", () => {
    expect(MORNING).toContain(pickGreeting({ awayHours: 3, hourOfDay: 9 }, () => 0.99));
    expect(NIGHT).toContain(pickGreeting({ awayHours: 3, hourOfDay: 23 }, () => 0.99));
    expect(DEEPNIGHT).toContain(pickGreeting({ awayHours: 3, hourOfDay: 2 }, () => 0.99));
  });

  it("afternoon/evening hours get no tod extras (base bucket only)", () => {
    // rng 0.99 would land in extras if any were merged; with none, the pick
    // comes from the base pool.
    const line = pickGreeting({ awayHours: 3, hourOfDay: 16 }, () => 0.99);
    expect(DAY).toContain(line);
  });

  it("never serves the same line twice in a row within a bucket", () => {
    let prev = "";
    for (let i = 0; i < 200; i++) {
      const line = pickGreeting({ awayHours: 14, hourOfDay: 10 });
      if (prev) expect(line).not.toBe(prev);
      prev = line;
    }
  });

  it("no-repeat memory is per bucket (switching buckets may repeat a line)", () => {
    const a = pickGreeting({ awayHours: 0.4, hourOfDay: 14 }, () => 0);
    // Different bucket, same forced index 0 — must still return its own line.
    expect(BRIEF).toContain(a);
    resetGreetingMemory();
    const b = pickGreeting({ awayHours: 3, hourOfDay: 14 }, () => 0);
    expect(DAY).toContain(b);
  });
});
