// Foley / interaction voice & foley sounds (design doc P11.5).
//
// Real sample files from public/audio/, chosen by WEIGHTED RANDOM per trigger.
// The previous version synthesized tones; we now ship the user's real Foley
// clips. Synthesis is retired — its job (a soft placeholder) is done.
//
// Design principles (北极星):
// - #5  Body-layer flourish: zero Mind/LLM dependency (offline OK).
// - #6  Muting supported; when muted, play()/preload() are no-ops.
// - #8  Zero recurring cost: local files, no API.
// - #10 "Less is more" — a sound is the EXCEPTION, not the rule. Every
//        trigger's largest weight is SILENCE, and BOTH sounding and silent
//        outcomes count against the cooldown, so rapid tapping can never
//        stack sounds. 宁少勿突兀.
// - #11 Every weight / cooldown / path / threshold is a named constant below;
//        tune the feel here without touching play().

export type TriggerId =
    | "pet-stranger" | "pet-intimate"
    | "poke1" | "poke2" | "poke3"
    | "drag" | "land" | "send" | "dblclick" | "menu";

/**
 * Closeness >= this switches head-petting from the reserved "stranger" voice
 * to the warm "intimate" voice. Tunable; backend closeness is 0..100.
 */
export const INTIMATE_THRESHOLD = 40;

// --- Asset keys -> public paths. Vite serves public/ at BASE_URL root. ---
type AssetKey =
    | "surprise-soft" | "startle-short" | "soft-ah" | "annoyed" | "laugh"
    | "cloth" | "land" | "lift" | "send" | "greeting";

// Public assets served at origin root (same convention as Live2DCanvas paths).
const ASSET_PATH: Record<AssetKey, string> = {
    "surprise-soft": "/audio/voice/surprise-soft.mp3", // ow: mild surprise / interrupted
    "startle-short": "/audio/voice/startle-short.mp3", // 啊1 short: poked startle
    "soft-ah":      "/audio/voice/soft-ah.mp3",        // 啊 longer: contented pet
    "annoyed":      "/audio/voice/annoyed.mp3",        // 生气: 3rd+ poke
    "laugh":        "/audio/voice/laugh.mp3",          // 笑: happy / intimate pet
    "cloth":        "/audio/foley/cloth.mp3",          // 布料声: fabric rustle
    "land":         "/audio/foley/land.mp3",           // 落地声: landing thud
    "lift":         "/audio/foley/lift.mp3",           // 跳: pick-up effort
    "send":         "/audio/ui/send.mp3",              // UI音效: send confirm
    "greeting":     "/audio/voice/greeting.mp3",       // hi: startup greeting
};

interface Variant {
    /** Asset to play. Omitted => SILENCE (the "do nothing" outcome). */
    key?: AssetKey;
    /** Relative probability weight. */
    w: number;
}

interface TriggerConfig {
    /** Min ms between attempts of this trigger. Both sounding AND silent reset it. */
    cooldownMs: number;
    variants: Variant[];
}

const TRIGGERS: Record<TriggerId, TriggerConfig> = {
    // 摸头 — intimacy-gated. Stranger: ~even silent / cloth / peep.
    "pet-stranger": { cooldownMs: 3000, variants: [
        { w: 50 }, { key: "cloth", w: 30 }, { key: "surprise-soft", w: 20 },
    ] },
    // 摸头 — intimate: she's okay being touched, may purr / laugh.
    "pet-intimate": { cooldownMs: 3000, variants: [
        { w: 30 }, { key: "soft-ah", w: 35 }, { key: "laugh", w: 20 }, { key: "cloth", w: 15 },
    ] },
    // 戳 1st: mild surprise.
    "poke1": { cooldownMs: 2000, variants: [
        { w: 25 }, { key: "surprise-soft", w: 45 }, { key: "startle-short", w: 30 },
    ] },
    // 戳 2nd: occasional short startle.
    "poke2": { cooldownMs: 2000, variants: [
        { w: 55 }, { key: "startle-short", w: 45 },
    ] },
    // 戳 3rd+: annoyed hum (the "stop poking me" beat).
    "poke3": { cooldownMs: 2000, variants: [
        { w: 25 }, { key: "annoyed", w: 75 },
    ] },
    // 抓起拖动: pick-up effort, often silent.
    "drag": { cooldownMs: 800, variants: [
        { w: 55 }, { key: "lift", w: 45 },
    ] },
    // 落地: thud is the main beat.
    "land": { cooldownMs: 600, variants: [
        { w: 15 }, { key: "land", w: 70 }, { key: "surprise-soft", w: 15 },
    ] },
    // 发送: UI confirm — always plays (explicit user action feedback).
    "send": { cooldownMs: 400, variants: [
        { key: "send", w: 100 },
    ] },
    // 双击打开对话: UI feedback — plays half the time (explicit action, but
    // not every single time, to avoid feeling mechanical).
    "dblclick": { cooldownMs: 1000, variants: [
        { w: 50 }, { key: "surprise-soft", w: 50 },
    ] },
    // 右键菜单打开: UI feedback — always plays (explicit action).
    "menu": { cooldownMs: 600, variants: [
        { key: "send", w: 100 },
    ] },
};

class SoundManager {
    private ctx: AudioContext | null = null;
    private muted = false;
    private buffers = new Map<AssetKey, Promise<AudioBuffer | null>>();
    private lastAttemptAt = new Map<TriggerId, number>();
    private greeted = false;    // startup "hi" has fired
    private greetArmed = false; // deferred-greet listener registered

    private ensureCtx(): AudioContext | null {
        if (this.muted) return null;
        if (!this.ctx) {
            const AC = window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
            if (!AC) return null;
            this.ctx = new AC();
        }
        if (this.ctx.state === "suspended") this.ctx.resume().catch(() => {});
        return this.ctx;
    }

    /** Preload all assets so the first interaction has no fetch delay. */
    preload(): void {
        const ctx = this.ensureCtx();
        if (!ctx) return; // muted or unavailable
        for (const key of Object.keys(ASSET_PATH) as AssetKey[]) this.load(key, ctx);
    }

    private load(key: AssetKey, ctx: AudioContext): Promise<AudioBuffer | null> {
        const cached = this.buffers.get(key);
        if (cached) return cached;
        const p = fetch(ASSET_PATH[key])
            .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(`${key}: HTTP ${r.status}`))))
            .then((buf) => ctx.decodeAudioData(buf))
            .catch((e) => { console.warn("[sound] load failed", key, e); return null; });
        this.buffers.set(key, p);
        return p;
    }

    /** Play one asset sample right now (shared by play() and greet()). */
    private playSample(key: AssetKey, ctx: AudioContext): void {
        this.load(key, ctx).then((buf) => {
            if (!buf || this.muted) return;
            const src = ctx.createBufferSource();
            src.buffer = buf;
            src.connect(ctx.destination);
            src.start();
            src.onended = () => { try { src.disconnect(); } catch { /* already gone */ } };
        });
    }

    /** Pick a variant by weight and play it. Respects cooldown + mute (#6). */
    play(trigger: TriggerId): void {
        const ctx = this.ensureCtx();
        if (!ctx) return;
        const cfg = TRIGGERS[trigger];

        // Cooldown: both sounding and silent outcomes reset it, so rapid taps
        // cannot stack sounds (the "less is more" guarantee, #10).
        const now = Date.now();
        const last = this.lastAttemptAt.get(trigger) ?? 0;
        if (now - last < cfg.cooldownMs) return;
        this.lastAttemptAt.set(trigger, now);

        const variant = this.pick(cfg.variants);
        if (!variant.key) return; // silence chosen — still counts (cooldown set above)
        this.playSample(variant.key, ctx);
    }

    /**
     * Startup greeting ("hi"). Plays exactly once. If the AudioContext is
     * suspended at launch (autoplay policy — no user gesture yet), defers to
     * the first interaction that wakes the context, so she always greets you.
     */
    greet(): void {
        if (this.greeted || this.greetArmed) return;
        const ctx = this.ensureCtx();
        if (!ctx) return; // muted / unavailable
        if (ctx.state === "running") { this.fireGreet(ctx); return; }
        this.greetArmed = true;
        const onDown = () => {
            const c = this.ctx;
            if (!c) return;
            c.resume().then(() => {
                if (this.greeted || c.state !== "running") return;
                window.removeEventListener("pointerdown", onDown);
                this.fireGreet(c);
            }).catch(() => {});
        };
        window.addEventListener("pointerdown", onDown);
    }

    private fireGreet(ctx: AudioContext): void {
        if (this.greeted) return;
        this.greeted = true;
        this.playSample("greeting", ctx);
    }

    private pick(variants: Variant[]): Variant {
        const total = variants.reduce((s, v) => s + v.w, 0);
        let r = Math.random() * total;
        for (const v of variants) {
            r -= v.w;
            if (r <= 0) return v;
        }
        return variants[variants.length - 1];
    }

    setMuted(muted: boolean): void { this.muted = muted; }
    isMuted(): boolean { return this.muted; }
    /** Toggle mute; returns the new muted state. */
    toggleMuted(): boolean { this.muted = !this.muted; return this.muted; }
}

export const sound = new SoundManager();
