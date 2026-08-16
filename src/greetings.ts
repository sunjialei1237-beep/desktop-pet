// Diversified local greeting pool for the restart/resume bubble (zero LLM —
// cost control, user request 2026-08-16: "打招呼语尽可能多元化，不走大模型").
// Voice: Liri — 安静、温柔、便签风短句、不黏人（docs/specs/liri 角色圣经）。
//
// Dimensions:
//   awayHours  — how long the app was closed (brief bounce / away for the day
//                / overnight sleep). Mirrors the old hardcoded
//                "我睡了N个小时……你回来啦~" one-liner it replaces.
//   hourOfDay  — local hour; overlays time-flavored variants on top of the
//                duration bucket's common lines.
// pickGreeting is pure apart from a module-level no-immediate-repeat memory
// (and takes an injectable rng for tests).

export interface GreetingContext {
  awayHours: number;
  hourOfDay: number; // 0-23 local
}

const BRIEF: string[] = [
  "咦，刚刚那下卡住了？",
  "我眨了下眼，你就回来了。",
  "嗯，在的。",
  "重启好啦，继续陪你。",
  "刚缓过来，没事了。",
];

const DAY: string[] = [
  "你回来啦。",
  "嗯，等你半天了。",
  "刚还在想你去哪了。",
  "回来啦，喝水了吗？",
  "嗯，回来了就好。",
  "我先把刚才的念头收好了。",
];

const OVERNIGHT: string[] = [
  "我睡了好久……你回来啦。",
  "睡了一觉，你一直没来。",
  "醒来啦，你呢？",
  "刚才做了个好长的梦。",
  "我睡饱了，你呢？",
];

// Time-of-day flavored extras, merged into the duration bucket when the hour
// matches. Kept small so the merged pool stays balanced.
const MORNING_EXTRAS: string[] = [
  "早呀，新的一天。",
  "这么早就来啦。",
  "早上好，吃早饭了吗？",
];
const NIGHT_EXTRAS: string[] = [
  "这么晚才来呀。",
  "夜里也想起我了？",
  "都这个点了，陪我一小会儿就好。",
];
const DEEPNIGHT_EXTRAS: string[] = [
  "……你怎么还没睡。",
  "这个点来，是睡不着吗？",
];

// No-immediate-repeat memory: the last line served per bucket key.
const lastServed: Record<string, string> = {};

function bucketKey(ctx: GreetingContext): string {
  if (ctx.awayHours < 1) return "brief";
  if (ctx.awayHours < 8) return "day";
  return "overnight";
}

function todExtras(hourOfDay: number): string[] {
  if (hourOfDay >= 0 && hourOfDay < 5) return DEEPNIGHT_EXTRAS;
  if (hourOfDay >= 23) return NIGHT_EXTRAS;
  if (hourOfDay >= 5 && hourOfDay < 11) return MORNING_EXTRAS;
  return [];
}

/// Pick a greeting line for the given context. `rng` defaults to Math.random;
/// tests inject a deterministic one. Never returns the same line twice in a
/// row for the same bucket (unless the bucket has a single line).
export function pickGreeting(ctx: GreetingContext, rng: () => number = Math.random): string {
  const key = bucketKey(ctx);
  const base = key === "brief" ? BRIEF : key === "day" ? DAY : OVERNIGHT;
  const pool = [...base, ...todExtras(ctx.hourOfDay)];
  const last = lastServed[key];
  // No-immediate-repeat: exclude the last line unless it's the only one.
  const from = last && pool.length > 1 ? pool.filter((l) => l !== last) : pool;
  const line = from[Math.floor(rng() * from.length)];
  lastServed[key] = line;
  return line;
}

/// Test aid: clear the no-repeat memory.
export function resetGreetingMemory(): void {
  for (const k of Object.keys(lastServed)) delete lastServed[k];
}
