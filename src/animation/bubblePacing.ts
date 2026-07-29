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
    "难过": { intervalMs: 50, catchDiv: 8, hesitate: 0.10 }, // 慢、一字一顿
    "疲惫": { intervalMs: 55, catchDiv: 9, hesitate: 0.15 }, // 很慢、带喘
};

/// Resolve typewriter pacing for a mood label. Unknown labels fall back to the
/// calm baseline.
export function typewriterPacing(moodLabel: string): TypewriterPacing {
    return PACING_BY_MOOD[moodLabel] ?? PACING_CALM;
}
