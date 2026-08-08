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

// The behavior table is pure data (weights / cooldowns / modifiers / circadian
// multipliers), kept in a sibling JSON asset so tuning does not touch logic.
// Loaded at build time via resolveJsonModule and cast to the typed shape below.
import idleBehaviorsData from "./idle-behaviors.json";

export const IDLE_BEHAVIORS: IdleBehavior[] = idleBehaviorsData as IdleBehavior[];

interface PickOptions {
    emotionMood: number;
    emotionEnergy: number;
    closeness: number;
    sleepiness: number;
    recentHistory: string[];
    cooldowns: Map<string, number>;
    now: number;
}

/**
 * Circadian sleepiness weight multiplier (design doc 6.7). Pure extraction of
 * the formula used in pickNextBehavior so the A5 effect — yawn/stretch climb at
 * night, look_around/peek fade, daytime untouched — is unit-testable without
 * the weighted-random machinery.
 *   effective = base * (1 + (sleepy - 1) * sleepiness)
 * - sleepy omitted => treated as 1 (time-invariant).
 * - sleepiness = 0 (day) => collapses to base (daytime mix byte-for-byte same).
 * Result is clamped to a tiny positive floor so a behavior can never hit zero
 * weight (keeps it in the pool, just very unlikely).
 */
export function applySleepyWeight(
    baseWeight: number,
    sleepy: number | undefined,
    sleepiness: number,
): number {
    const s = sleepy ?? 1;
    return Math.max(0.01, baseWeight * (1 + (s - 1) * sleepiness));
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
        return applySleepyWeight(w, b.sleepy, sleepiness);
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
