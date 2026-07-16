// Microbehavior system: weighted random idle behaviors with cooldowns.
// Design doc 6.2: Idle Variety = weighted random + Cooldown + Recent History avoidance.

export interface IdleBehavior {
    name: string;
    weight: number;
    cooldown_ms: number;
    emotion_modifier: Record<string, number>;
    min_closeness: number;
}

export const IDLE_BEHAVIORS: IdleBehavior[] = [
    { name: "blink", weight: 3.0, cooldown_ms: 3000, emotion_modifier: {}, min_closeness: 0 },
    { name: "look_around", weight: 2.0, cooldown_ms: 15000, emotion_modifier: { happy: 1.5, curious: 2.0 }, min_closeness: 0 },
    { name: "tilt_head", weight: 1.5, cooldown_ms: 12000, emotion_modifier: { curious: 2.0 }, min_closeness: 0 },
    { name: "yawn", weight: 0.8, cooldown_ms: 60000, emotion_modifier: { tired: 3.0 }, min_closeness: 0 },
    { name: "stretch", weight: 1.0, cooldown_ms: 45000, emotion_modifier: { happy: 1.5 }, min_closeness: 0 },
    { name: "sway", weight: 0.7, cooldown_ms: 30000, emotion_modifier: { happy: 2.0 }, min_closeness: 10 },
    { name: "hum", weight: 0.5, cooldown_ms: 60000, emotion_modifier: { happy: 2.5 }, min_closeness: 20 },
    { name: "peek", weight: 0.8, cooldown_ms: 20000, emotion_modifier: { curious: 1.5 }, min_closeness: 0 },
];

interface PickOptions {
    emotionMood: number;
    emotionEnergy: number;
    closeness: number;
    recentHistory: string[];
    cooldowns: Map<string, number>;
    now: number;
}

/// Picks the next microbehavior based on emotion, closeness, cooldowns, and history.
export function pickNextBehavior(opts: PickOptions): string | null {
    const { emotionMood, emotionEnergy, closeness, recentHistory, cooldowns, now } = opts;

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
