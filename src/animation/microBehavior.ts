// Microbehavior system: weighted random idle behaviors with cooldowns.
// Design doc 6.2: Idle Variety = weighted random + Cooldown + Recent History avoidance.

export interface IdleBehavior {
    name: string;
    weight: number;
    cooldown_ms: number;
    emotion_modifier: Record<string, number>;
    min_closeness: number;
    /**
     * Circadian sleepiness weight multiplier (design doc 6.7). The effective
     * weight is interpolated by the live sleepiness value:
     *   w *= 1 + (sleepy - 1) * sleepiness
     * - omitted / 1.0 => unaffected by time of day
     * - > 1 (yawn/stretch) => more likely at night (drowsy signals)
     * - < 1 (look_around/peek) => suppressed at night (low-energy fidgets)
     * At sleepiness=0 (daytime) every multiplier collapses to 1, so daytime
     * behavior is byte-for-byte unchanged (backward compatible).
     */
    sleepy?: number;
}

export const IDLE_BEHAVIORS: IdleBehavior[] = [
    { name: "blink", weight: 3.0, cooldown_ms: 3000, emotion_modifier: {}, min_closeness: 0 },
    { name: "look_around", weight: 2.0, cooldown_ms: 15000, emotion_modifier: { happy: 1.5, curious: 2.0 }, min_closeness: 0, sleepy: 0.3 },
    { name: "tilt_head", weight: 1.5, cooldown_ms: 12000, emotion_modifier: { curious: 2.0 }, min_closeness: 0, sleepy: 0.6 },
    { name: "yawn", weight: 0.8, cooldown_ms: 60000, emotion_modifier: { tired: 3.0 }, min_closeness: 0, sleepy: 5.0 },
    { name: "stretch", weight: 1.0, cooldown_ms: 45000, emotion_modifier: { happy: 1.5 }, min_closeness: 0, sleepy: 2.0 },
    { name: "sway", weight: 0.7, cooldown_ms: 30000, emotion_modifier: { happy: 2.0 }, min_closeness: 10, sleepy: 0.5 },
    { name: "hum", weight: 0.5, cooldown_ms: 60000, emotion_modifier: { happy: 2.5 }, min_closeness: 20, sleepy: 0.8 },
    { name: "peek", weight: 0.8, cooldown_ms: 20000, emotion_modifier: { curious: 1.5 }, min_closeness: 0, sleepy: 0.4 },
];

interface PickOptions {
    emotionMood: number;
    emotionEnergy: number;
    closeness: number;
    sleepiness: number;
    recentHistory: string[];
    cooldowns: Map<string, number>;
    now: number;
}

/// Picks the next microbehavior based on emotion, closeness, cooldowns, and history.
export function pickNextBehavior(opts: PickOptions): string | null {
    const { emotionMood, emotionEnergy, closeness, sleepiness, recentHistory, cooldowns, now } = opts;

    const pool = IDLE_BEHAVIORS.filter((b) => {
        // Filter by cooldown
        const last = cooldowns.get(b.name) || 0;
        if (now - last < b.cooldown_ms) return false;
        // Filter by closeness
        if (closeness < b.min_closeness) return false;
        // Filter by recent history (avoid repeating last 5)
        if (recentHistory.includes(b.name)) return false;
        return true;
    });

    if (pool.length === 0) return null;

    // Compute emotion-based modifier
    const moodLabel = emotionMood > 0.6 ? "happy" : emotionMood < 0.35 ? "sad" : "neutral";
    const energyLabel = emotionEnergy < 0.3 ? "tired" : "energetic";

    const weights = pool.map((b) => {
        let w = b.weight;
        const mod = b.emotion_modifier;
        if (mod[moodLabel]) w *= mod[moodLabel];
        if (mod[energyLabel]) w *= mod[energyLabel];
        // Curious boost when low closeness (getting to know user)
        if (closeness < 10 && mod["curious"]) w *= 1.5;
        // Circadian sleepiness (design doc 6.7): at night she gets drowsy --
        // yawn/stretch climb, look_around/peek fade. sleepiness=0 (day) is a
        // no-op (multiplier collapses to 1), so daytime mix is unchanged.
        const sleepy = b.sleepy ?? 1;
        w *= 1 + (sleepy - 1) * sleepiness;
        return Math.max(0.01, w);
    });

    // Weighted random pick
    const total = weights.reduce((a, b) => a + b, 0);
    let r = Math.random() * total;
    for (let i = 0; i < pool.length; i++) {
        r -= weights[i];
        if (r <= 0) return pool[i].name;
    }
    return pool[pool.length - 1].name;
}
