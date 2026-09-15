// Spine intent translation layer for Liri.
// Maps FSM BehaviorState / life timers → Spine track ops, and runs EMOTION
// PROGRAMS: named composites of parallel channel members (per the user-approved
// combination plan, 2026-08-27). Contract: docs/specs/liri/animation_spec.md
// (17-animation edition), skeleton_structure.md.
//
// ARCHITECTURE — three mechanisms (v2 asset, 17 animations; calm-idle tuned
// per user 2026-08-28):
//
// 1. DISJOINT BONE DOMAINS. liriAssetPatch strips the cross-domain flatline
//    pins so every concurrently-playable track owns a disjoint bone set:
//      skirt/arm own their parts; ear owns ear_l2/ear_r2; tail owns tail_1..5;
//      hair owns hair chains + bangs; breath owns head/spine chain/ribbons.
// 2. CALM-IDLE BASE. Only breath (+ subtle skirt/arm ambience) loops forever.
//    Ear/hair/tail fire as SPORADIC ONE-SHOTS — never more often than every
//    15s (user rule), picked at random — so idle reads as: breathing sway,
//    occasionally a ear twitch / hair sway / tail wave. Emotion programs are
//    special cases ("另算") fired by events, never by the idle randomizer.
// 3. BREATH-ALIGNED PROGRAMS. Emotion programs (sad/happy/curious/thing)
//    START at a body_breath loop boundary (track0 `complete`) and END exactly
//    n boundaries later — all member channels revert in parallel on a beat, per
//    the rule 「所有动作在一个完整的呼吸动作开始时并行结束」.

import { BehaviorState } from "./fsm";

// Track layout, bottom → top. Higher index wins per keyed property. With the
// patch's disjoint domains this order is belt-and-braces, not correctness.
// ear/tail/hair are EMPTY at boot — they only carry sporadic one-shot part
// actions (the calm-idle rule: no part action more often than every 15s) and
// program members while a special emotion runs.
export const TRACK = {
  breath: 0, // body_breath — looping base life (spine sway/head bob/ribbons)
  skirt: 1, // Skirt_l — slow skirt flutter (the one permanent ambient)
  hair: 2, // hair_idle one-shot (sporadic)
  arm: 3, // arm_idle one-shot (sporadic — 用户: 手臂不要常驻)
  ear: 4, // ear actions / program members
  tail: 5, // tail actions / program members (kept above ear: tail_1 ownership)
  gesture: 6, // thing (+ future touch reactions) — one-shot above everything
  expr: 7, // blink/wink/smile/eye_sad — serial one-shot queue
} as const;

export type VariantChannel = "ear" | "tail";
export type ProgramChannel = VariantChannel | "expr" | "gesture";

// ── Durations (parsed from v2 public/spine/liri/liri.json; pinned by test) ──
export const SECONDS = {
  breath: 4.3333, // one full body_breath cycle = one "beat"
  smile: 3.9333,
  eyeSad: 2.0,
  thing: 0.3333,
} as const;

// Expression re-trigger cadence while a sad program is active: restart eye_sad
// just past its end so the 难过眼/难过嘴 attachments never visibly drop between
// loops (attachment switch is discrete; the ~50ms gap is invisible).
const EYE_SAD_RETRIGGER = SECONDS.eyeSad + 0.05;

// Programs: named emotion composites ("特殊情况，另算" — fired by events/the
// future emotion bridge, never by the idle randomizer). beats × SECONDS.breath
// is the window; every member ends with the window at the next breath boundary.
//
//   sad        ear_sad ×1 (self-returning hold) + tail_sad loop + sustained
//              难过脸 (eye_sad re-triggered)                 → 8.67s window
//   happyLong  tail_happy loop + smile at start & midbeat  → 8.67s
//   happyShort tail_happy loop + smile once                → 4.33s
//   curious    ear_2 twitch + tail_2 lively swing          → 4.33s
//   thing      hands-up gesture (kept pose, code blends it back down) → 4.33s
export interface ProgramMember {
  channel: ProgramChannel;
  /** Loop-variant channels: swap this channel's loop animation. */
  anim?: string;
  /**
   * Variant-channel playback mode. Default true (loop through the window).
   * loop:false plays ONCE and holds its last frame — used by ear_sad, whose
   * final frame IS the neutral base (a self-returning one-shot), so holding
   * reads as "she did her sad ears, now she rests" (the approved recipe), not
   * a droop repeating like a metronome.
   */
  loop?: boolean;
  /** expr/gesture members fire as ONE-SHOT this many beats into the window. */
  triggerAtBeat?: number;
}

export interface ProgramDef {
  id: string;
  beats: number;
  members: ProgramMember[];
}

export const PROGRAMS: Record<string, ProgramDef> = {
  sad: {
    id: "sad",
    beats: 2,
    members: [
      { channel: "ear", anim: "ear_sad", loop: false },
      { channel: "tail", anim: "tail_sad" },
      { channel: "expr", anim: "eye_sad" }, // first fire at beat 0
    ],
  },
  happyLong: {
    id: "happyLong",
    beats: 2,
    members: [
      { channel: "tail", anim: "tail_happy" },
      { channel: "expr", anim: "smile", triggerAtBeat: 0 },
      { channel: "expr", anim: "smile", triggerAtBeat: 1 },
    ],
  },
  happyShort: {
    id: "happyShort",
    beats: 1,
    members: [
      { channel: "tail", anim: "tail_happy" },
      { channel: "expr", anim: "smile", triggerAtBeat: 0 },
    ],
  },
  curious: {
    id: "curious",
    beats: 1,
    members: [
      { channel: "ear", anim: "ear_2" },
      { channel: "tail", anim: "tail_2" },
    ],
  },
  thing: {
    id: "thing",
    beats: 1,
    members: [{ channel: "gesture", anim: "thing" }],
  },
};

// Part one-shot actions (the calm-idle randomizer's palette). All four are
// self-returning one-shots on their own (empty-at-boot) tracks — they own
// disjoint bones, so they fire freely between breath cycles without alignment.
export type PartAction = "ear" | "hair" | "tail" | "arm";
export const PART_ACTIONS: Record<PartAction, string> = {
  ear: "ear_idle",
  hair: "hair_idle",
  tail: "tail_idle",
  arm: "arm_idle",
};
export const PART_ACTION_DURATION: Record<PartAction, number> = {
  ear: 3.1,
  hair: 2.7,
  tail: 1.2,
  arm: 1.13,
};

export const GESTURE_FADE = 0.35; // setEmptyAnimation mix for the gesture track
export const IDLE_FADE = 0.3; // fade-back mix for part actions / variant channels

/// Set transition (mix) times once on the AnimationStateData (animation_spec §Mix).
export function setupMix(stateData: any) {
  stateData.defaultMix = 0.15; // every swap/fade eases in/out instead of snapping
  ["blink", "wink_L", "wink_R", "smile", "eye_sad"].forEach((a) =>
    stateData.setMixByName(a, a, 0.12),
  );
}

/// One-time: lay down the PERMANENT base loops only. Per the calm-idle rule
/// (2026-08-28 user) the base is breath sway + subtle skirt ambience only;
/// ear/hair/tail/arm all fire as sporadic one-shots instead of looping
/// (用户 2026-08-28 续：手臂也不要常驻).
export function setupIdleTracks(spine: any) {
  spine.state.setAnimation(TRACK.breath, "body_breath", true);
  spine.state.setAnimation(TRACK.skirt, "Skirt_l", true);
}

/// Fire a sporadic part action (one-shot; the canvas fades the track back out
/// via setEmptyAnimation once PART_ACTION_DURATION is nearly spent).
export function firePartAction(spine: any, part: PartAction): void {
  spine.state.setAnimation(TRACK[part], PART_ACTIONS[part], false);
}

// ── Program runner (pure-ish state; canvas drives it from updateFn/timers) ──

export interface RunnerState {
  activeDef: ProgramDef | null;
  elapsed: number; // wall-clock seconds since program start
  beatsDone: number; // breath boundaries elapsed since the current program started
  nextEyeSadAt: number | null; // wall-clock offset for the next eye_sad re-fire
  pendingBeatsFired: Set<number>; // one-shot members already fired
}

export function createProgramRunner(): RunnerState {
  return {
    activeDef: null,
    elapsed: 0,
    beatsDone: 0,
    nextEyeSadAt: null,
    pendingBeatsFired: new Set(),
  };
}

// Expression one-shot lengths, for spacing the serial expr queue (a second
// setAnimation would CUT a playing one — they share the track).
export const EXPR_DURATIONS: Record<string, number> = {
  blink: 0.1,
  wink_L: 0.1,
  wink_R: 0.1,
  smile: SECONDS.smile,
  eye_sad: SECONDS.eyeSad,
};

export function isProgramActive(runner: RunnerState): boolean {
  return runner.activeDef !== null;
}

function startProgram(spine: any, runner: RunnerState, def: ProgramDef): void {
  runner.activeDef = def;
  runner.elapsed = 0;
  runner.beatsDone = 0;
  runner.nextEyeSadAt = null;
  runner.pendingBeatsFired = new Set();
  // Pre-compute the eye_sad retrigger schedule (interval fires handled in tick).
  if (def.members.some((m) => m.anim === "eye_sad")) {
    runner.nextEyeSadAt = EYE_SAD_RETRIGGER;
  }
  applyProgramStart(spine, def);
}

function applyProgramStart(spine: any, def: ProgramDef): void {
  for (const m of def.members) {
    if (!m.anim) continue;
    if (m.channel === "expr") {
      if ((m.triggerAtBeat ?? 0) === 0) fireExpression(spine, m.anim);
    } else if (m.channel === "gesture") {
      spine.state.setAnimation(TRACK.gesture, m.anim, false);
    } else {
      // Loop-variant channel swap. Loop=true keeps the channel alive through
      // the whole window (seams are safe: these variants start/end on their
      // base values); loop=false (self-returning one-shots like ear_sad) plays
      // once and holds its neutral last frame until the window closes.
      spine.state.setAnimation(TRACK[m.channel], m.anim, m.loop !== false);
    }
  }
}

function fireExpression(spine: any, anim: string): void {
  spine.state.setAnimation(TRACK.expr, anim, false);
}

/**
 * Advance the runner by dt wall-clock seconds: fire scheduled one-shot members
 * (mid-window smile repeats), keep the sad face alive via eye_sad re-triggers.
 * Returns without side effects when no program is running.
 */
export function tickProgram(spine: any, runner: RunnerState, def: ProgramDef | null, dt: number): void {
  if (!def || runner.activeDef === null) return;
  runner.elapsed += dt;

  // Scheduled one-shot expression members (e.g. happyLong's second smile at
  // beat 1). Trigger offsets convert to seconds at fire time.
  for (const m of def.members) {
    if (m.channel !== "expr" || !m.anim || !(m.triggerAtBeat && m.triggerAtBeat > 0)) continue;
    if (runner.pendingBeatsFired.has(m.triggerAtBeat)) continue;
    const at = m.triggerAtBeat * SECONDS.breath;
    if (runner.elapsed >= at - 1 / 60) {
      runner.pendingBeatsFired.add(m.triggerAtBeat);
      fireExpression(spine, m.anim);
    }
  }

  // Sustained sad face: re-fire eye_sad every EYE_SAD_RETRIGGER seconds.
  if (runner.nextEyeSadAt !== null && runner.elapsed >= runner.nextEyeSadAt) {
    const windowLeft = def.beats * SECONDS.breath - runner.elapsed;
    if (windowLeft > SECONDS.eyeSad * 0.5) {
      fireExpression(spine, "eye_sad");
      runner.nextEyeSadAt += EYE_SAD_RETRIGGER;
    } else {
      runner.nextEyeSadAt = null; // too close to the window edge — let it rest
    }
  }
}

/**
 * Revert all channels touched by `def` back to their idle variants — called ON
 * the closing breath boundary so every member ends in parallel, aligned with
 * a complete breathing cycle starting fresh.
 */
export function finishProgram(spine: any, runner: RunnerState, def: ProgramDef | null): void {
  runner.activeDef = null;
  runner.elapsed = 0;
  runner.beatsDone = 0;
  runner.nextEyeSadAt = null;
  runner.pendingBeatsFired.clear();
  if (!def) return;
  const touched = new Set<VariantChannel>();
  for (const m of def.members) {
    if (m.channel === "ear" || m.channel === "tail") touched.add(m.channel);
    else if (m.channel === "gesture") {
      // thing has no return keys (ends holding the raised pose); fade the
      // track out over GESTURE_FADE so the arms lower smoothly onto whatever
      // the live tracks show at this moment.
      spine.state.setEmptyAnimation(TRACK.gesture, GESTURE_FADE);
    }
  }
  for (const ch of touched) {
    // No idle loops beneath anymore (calm-idle rule): fade the channel back to
    // empty so the body returns to breath-only base with the part at rest.
    spine.state.setEmptyAnimation(TRACK[ch], IDLE_FADE);
  }
}

/** Request the next program (queued; starts on the next breath boundary). */
let queued: ProgramDef | null = null;
export function requestProgram(id: string): boolean {
  if (!PROGRAMS[id] || queued) return false;
  queued = PROGRAMS[id];
  return true;
}

/**
 * ONE sync point with the breath clock — call from track0's `complete` listener.
 * Closes an active program whose window just ended (all members revert in
 * parallel), then starts the queued program, if any, exactly when body_breath
 * loops back to its initial pose. Returns the id of a program that finished
 * (debug/logging only).
 */
export function breathBoundary(spine: any, runner: RunnerState): string | null {
  let finishedId: string | null = null;
  if (runner.activeDef) {
    runner.beatsDone++;
    if (runner.beatsDone >= runner.activeDef.beats) {
      finishedId = runner.activeDef.id;
      finishProgram(spine, runner, runner.activeDef);
    }
  }
  if (!runner.activeDef && queued) {
    const next = queued;
    queued = null;
    startProgram(spine, runner, next);
  }
  return finishedId;
}

/** Wall-clock guard for tests/debug: how many beats a program needs. */
export function programWindow(def: ProgramDef): number {
  return def.beats * SECONDS.breath;
}

// ── FSM behavior → expression (unchanged semantics from v1) ──
// Embarrassed winks; blinking is physiological (fixed timer), not the FSM's
// scattered Blink state; other behaviors leave the track alone. Suppressed
// while a program is active (the serial discipline extends to expressions).
export function triggerBehavior(spine: any, behavior: BehaviorState, programRunning: boolean): boolean {
  if (programRunning) return false;
  switch (behavior) {
    case BehaviorState.Embarrassed:
      spine.state.setAnimation(
        TRACK.expr,
        Math.random() < 0.5 ? "wink_L" : "wink_R",
        false,
      );
      return true;
    default:
      return false;
  }
}

// ── Random intervals (seconds, wall-clock) ──
export function nextBlinkDelay(): number {
  return 4 + Math.random() * 2; // ~5s human cadence
}
export function nextSmileDelay(): number {
  return 12 + Math.random() * 6; // 12-18s sparse warmth
}
/// Part-action cadence: never more often than every 15s (user calm-idle rule),
/// with random stretch to 25s so she doesn't tick like a clock.
export function nextPartDelay(): number {
  return 15 + Math.random() * 10;
}
/// Uniform pick among the three idle part actions.
export function pickPartAction(): PartAction {
  const parts: PartAction[] = ["ear", "hair", "tail"];
  return parts[Math.floor(Math.random() * parts.length)];
}
