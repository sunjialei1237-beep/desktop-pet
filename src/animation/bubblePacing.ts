// Typewriter pacing per mood — bubble liveliness (Tier 1 #2, architecture #10).
//
// The streaming chat bubble reveals buffered content on an interval. A single
// fixed cadence throws away *how* she is speaking: a happy reply should flow
// fast, a sad/worried one should drag with pauses. This pure function maps the
// mood label (the same one driving the bubble's shape class via
// `bubbleClassForMood`) to three knobs:
//
//  - intervalMs : tick period. Lower = faster reveal.
//  - catchDiv   : "catch-up" divisor on the buffered-vs-shown gap. Lower =
//                 bigger steps per tick, so a fast stream still keeps up.
//                 step = max(1, ceil(gap / catchDiv))
//  - hesitate   : chance [0..1] a tick is skipped (a pause) WHILE the stream
//                 is still open — conveys hesitation / catching breath. The
//                 caller MUST NOT apply hesitate once the stream has ended;
//                 the final reveal must always complete.
//
// Pure (architecture #11): same mood -> same pacing, no side effects, trivially
// unit-testable. Architecture #1: rules only, no LLM.

export interface TypewriterPacing {
    intervalMs: number;
    catchDiv: number;
    hesitate: number;
}

// Calm baseline — also the fallback for unknown / brand-new labels so a stray
// mood label never breaks the bubble.
const PACING_CALM: TypewriterPacing = { intervalMs: 32, catchDiv: 5, hesitate: 0 };

// Mood label -> pacing. Tuned so happy/playful read as clearly brisk and
// sad/tired as clearly slow with pauses. All knobs adjustable here in one place.
const PACING_BY_MOOD: Record<string, TypewriterPacing> = {
    "开心": { intervalMs: 22, catchDiv: 3, hesitate: 0 },    // 快、流畅
    "调皮": { intervalMs: 26, catchDiv: 3, hesitate: 0 },    // 快、活泼
    "平静": PACING_CALM,                                     // baseline
    "担心": { intervalMs: 42, catchDiv: 7, hesitate: 0.20 }, // 慢、常停顿（犹豫）
    "难过": { intervalMs: 72, catchDiv: 10, hesitate: 0.20 }, // 慢、一字一顿（v2 调慢）
    "疲惫": { intervalMs: 55, catchDiv: 9, hesitate: 0.15 }, // 很慢、带喘
};

/// Resolve typewriter pacing for a mood label. Unknown labels fall back to the
/// calm baseline.
export function typewriterPacing(moodLabel: string): TypewriterPacing {
    return PACING_BY_MOOD[moodLabel] ?? PACING_CALM;
}

// Keyword -> pacing-mood inference from the user's input text.
//
// Why not just use the backend moodLabel? mood is a SLOW variable: the backend
// only re-derives emotion in converse Step 12, AFTER the LLM streams (Step 9),
// and one sad turn barely moves it. At first-chunk time moodLabel is still last
// turn's value, so "I'm sad" never reaches this turn's cadence. The user's own
// words are the only immediate signal available when the first chunk arrives.
//
// This is a UI-rhythm heuristic only — no LLM, no state written (architecture
// #1 respected). Follow-up: have the backend send a single pacing/emotion hint
// before the stream starts, so this stops duplicating react.rs's job.
const PACING_KEYWORDS: Record<string, string[]> = {
    "难过": ["难过", "难過", "伤心", "傷心", "难受", "難受", "哭", "去世", "走了", "没了", "失去", "逝世", "死了", "心痛", "低落", "郁闷", "崩溃"],
    "担心": ["担心", "擔心", "焦虑", "害怕", "紧张", "不安", "心慌"],
    "疲惫": ["好累", "很累", "太累", "疲惫", "犯困", "困了", "熬夜", "撑不住", "没力气", "精疲力尽"],
    "开心": ["开心", "高興", "高兴", "哈哈", "嘻嘻", "嘿嘿", "好玩", "有趣", "太棒", "好棒", "谢谢", "謝謝", "快乐", "好耶"],
    "调皮": ["调皮", "調皮", "逗你", "恶作剧", "捉弄"],
};

/// Infer the pacing mood from the user's input text, falling back to the
/// backend mood label when no emotion keyword is present. Returns one of the
/// keys understood by `typewriterPacing` (or the fallback verbatim).
export function inferPacingMood(text: string, fallback: string): string {
    for (const [mood, words] of Object.entries(PACING_KEYWORDS)) {
        if (words.some((w) => text.includes(w))) return mood;
    }
    return fallback;
}
