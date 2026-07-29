// Animation Finite State Machine.
// Design doc 6.1: Behavior States drive animations, not the other way around.
// Priority system: some states can interrupt others, some cannot.

export enum BehaviorState {
    Idle = "idle",
    Blink = "blink",
    LookAround = "look_around",
    TiltHead = "tilt_head",
    Yawn = "yawn",
    Stretch = "stretch",
    Sway = "sway",
    Hum = "hum",
    Peek = "peek",
    Talking = "talking",
    Thinking = "thinking",
    Sleeping = "sleeping",
    Embarrassed = "embarrassed",
    Recovering = "recovering",
}

// States that can be interrupted by other behaviors
const INTERRUPTIBLE = new Set<BehaviorState>([
    BehaviorState.Idle,
    BehaviorState.Blink,
    BehaviorState.LookAround,
    BehaviorState.TiltHead,
    BehaviorState.Yawn,
    BehaviorState.Stretch,
    BehaviorState.Sway,
    BehaviorState.Hum,
    BehaviorState.Peek,
]);

// How long each microbehavior lasts (ms) before returning to Idle
const BEHAVIOR_DURATION: Record<string, number> = {
    blink: 800,
    look_around: 2500,
    tilt_head: 1500,
    yawn: 2000,
    stretch: 2000,
    sway: 3000,
    hum: 3000,
    peek: 1500,
};

export class AnimationFSM {
    private current: BehaviorState = BehaviorState.Idle;
    private history: string[] = [];
    private cooldowns = new Map<string, number>();
    private behaviorEndTime = 0;
    private listeners: ((state: BehaviorState) => void)[] = [];

    get state(): BehaviorState {
        return this.current;
    }

    onStateChange(cb: (state: BehaviorState) => void) {
        this.listeners.push(cb);
        return () => {
            this.listeners = this.listeners.filter((l) => l !== cb);
        };
    }

    private setState(state: BehaviorState) {
        if (this.current === state) return;
        this.current = state;
        this.listeners.forEach((cb) => cb(state));
    }

    /// Transitions to a new state if allowed by priority rules.
    transition(to: BehaviorState) {
        if (INTERRUPTIBLE.has(this.current) || !INTERRUPTIBLE.has(to)) {
            this.setState(to);
            if (BEHAVIOR_DURATION[to]) {
                this.behaviorEndTime = Date.now() + BEHAVIOR_DURATION[to];
            }
        }
    }

    /// Called every tick. Returns to Idle when behavior ends, then picks next microbehavior.
    tick(
        emotionMood: number,
        emotionEnergy: number,
        closeness: number,
        sleepiness: number,
        now: number,
        pickBehavior: (opts: {
            emotionMood: number;
            emotionEnergy: number;
            closeness: number;
            sleepiness: number;
            recentHistory: string[];
            cooldowns: Map<string, number>;
            now: number;
        }) => string | null,
    ) {
        // If in a timed microbehavior and it's over, return to Idle
 if (this.current !== BehaviorState.Idle &&
            this.current !== BehaviorState.Talking &&
            this.current !== BehaviorState.Thinking &&
            this.current !== BehaviorState.Sleeping &&
            now >= this.behaviorEndTime) {
            // Record in history
            this.history.push(this.current);
            if (this.history.length > 5) this.history.shift();
            // Set cooldown
            this.cooldowns.set(this.current, now);
            this.setState(BehaviorState.Idle);
        }

        // If Idle, maybe pick a new microbehavior (not every tick, check timing)
        if (this.current === BehaviorState.Idle) {
            const behavior = pickBehavior({
                emotionMood,
                emotionEnergy,
                closeness,
                sleepiness,
                recentHistory: this.history,
                cooldowns: this.cooldowns,
                now,
            });
            if (behavior) {
                this.transition(behavior as BehaviorState);
            }
        }
    }

    /// Force a state (used for Talking, Thinking, etc.)
    forceState(state: BehaviorState) {
        this.setState(state);
    }
}
