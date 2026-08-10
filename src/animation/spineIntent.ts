// Spine intent translation layer for Liri.
// Maps FSM BehaviorState → Spine track / slot ops, plus a single serial action
// channel for sporadic life. Replaces the Live2D Cubism-param translation
// (emotionDriver/behaviorDriver wrote ParamEyeLOpen etc.); intent sources
// (FSM/circadian/EmotionVector) are reused, only the parameter layer is
// rewritten to Spine. Contract: docs/specs/liri/animation_spec.md,
// skeleton_structure.md.

import { BehaviorState } from "./fsm";

// Track layout. body_breath (track0) is the ONLY continuously-looping track
// (base life). ear/tail idle fire as ONE-SHOT events on their own tracks and
// never loop. The expression track sits highest so its slot keys beat all.
const TRACK = {
  breath: 0, // body_breath (continuous base life)
  ear: 1, // ear_idle one-shot
  tail: 2, // tail_idle one-shot
  expr: 5, // blink / wink / smile (transient, one at a time)
};

// WHY ONE-SHOT, NOT LOOP: every Liri idle animation (ear/tail/arm/hair) keys
// the whole spine chain + head, not just its named part. Looping any of them
// yanks the body sideways and snaps it back at the loop seam (the "身体摆到右→
// 跳到最左" jump). A one-shot plays once (first frame IS setup), then
// setEmptyAnimation fades smoothly back to setup — zero jump at entry/exit.
//
// WHY BREATH-ALIGNED: ear/tail also key the spine chain, so they override
// body_breath while active. Firing mid-breath makes the body jump from the
// breath's mid-cycle pose to the idle's first frame. So ear/tail only fire at
// body_breath's loop boundary (each `complete`), where the body is back at
// setup — the idle's first frame then matches the current pose, no jump.
// blink/smile key only eye SLOTS, never the spine, so they need no alignment.
export type ActionKind = "ear" | "tail" | "blink" | "smile";

export const IDLE_FADE = 0.3; // setEmptyAnimation mix: smooth return to setup
const EAR_SECONDS = 1.0; // ear_idle one-shot length (liri.json: 1.0s)
const TAIL_SECONDS = 1.1; // tail_idle one-shot length (1.07s)

/// Set transition (mix) times once on the AnimationStateData (animation_spec §Mix).
export function setupMix(stateData: any) {
  // Global crossfade so one-shot idles ease in/out from setup rather than snap.
  stateData.defaultMix = 0.15;
  ["blink", "wink_L", "wink_R", "smile"].forEach((a) =>
    stateData.setMixByName(a, a, 0.12),
  );
}

/// One-time: lay down the continuous base breath track ONLY.
export function setupIdleTracks(spine: any) {
  spine.state.setAnimation(TRACK.breath, "body_breath", true);
}

/// FSM behavior → expression. Only Embarrassed drives an anim (single-eye wink
/// stands in — no dedicated anim). Blinking is physiological (fixed timer),
/// not the FSM's scattered Blink state; other behaviors leave the track alone.
export function triggerBehavior(spine: any, behavior: BehaviorState) {
  switch (behavior) {
    case BehaviorState.Embarrassed:
      spine.state.setAnimation(
        TRACK.expr,
        Math.random() < 0.5 ? "wink_L" : "wink_R",
        false,
      );
      break;
    default:
      break;
  }
}

// ── Face (smile mouth-slot override) ──
// smile.json keys only EYE slots (笑眯眼) — the mouth is never touched, so the
// smile animation alone leaves the default mouth on. We manually swap 嘴→hidden
// and 小笑嘴→shown for the smile's duration. initFace captures refs at setup.
export interface FaceState {
  mouthSlot: any;
  smileMouthSlot: any;
  defaultMouthAtt: any;
  smileMouthAtt: any;
  smileDuration: number;
}

export function initFace(spine: any): FaceState | null {
  try {
    const sk = spine.skeleton;
    const mouthSlot = sk.findSlot("嘴");
    const smileMouthSlot = sk.findSlot("小笑嘴");
    if (!mouthSlot || !smileMouthSlot) return null;
    const smileMouthAtt = sk.getAttachmentByName("小笑嘴", "小笑嘴");
    const smileAnim = sk.data.findAnimation("smile");
    return {
      mouthSlot,
      smileMouthSlot,
      defaultMouthAtt: mouthSlot.attachment,
      smileMouthAtt,
      smileDuration: smileAnim ? smileAnim.duration : 1.5,
    };
  } catch {
    return null;
  }
}

// ── Single serial action channel ──
// The scheduler (SpineCanvas) fires ONE action at a time behind a shared busy
// flag, so blink/ear/tail/smile never overlap. ear/tail additionally wait for a
// body_breath loop boundary (breath-aligned) to avoid spine-chain jumps.
function playEar(spine: any) {
  spine.state.setAnimation(TRACK.ear, "ear_idle", false);
}
function playTail(spine: any) {
  spine.state.setAnimation(TRACK.tail, "tail_idle", false);
}
function triggerBlink(spine: any) {
  spine.state.setAnimation(TRACK.expr, "blink", false);
}
function triggerSmile(spine: any, face: FaceState | null) {
  spine.state.setAnimation(TRACK.expr, "smile", false);
  if (face) {
    face.mouthSlot.setAttachment(null);
    face.smileMouthSlot.setAttachment(face.smileMouthAtt);
  }
}
function endSmileMouth(face: FaceState | null) {
  if (!face) return;
  face.mouthSlot.setAttachment(face.defaultMouthAtt);
  face.smileMouthSlot.setAttachment(null);
}

/// Start a one-shot action on its track.
export function playAction(spine: any, kind: ActionKind, face: FaceState | null) {
  switch (kind) {
    case "ear": playEar(spine); break;
    case "tail": playTail(spine); break;
    case "blink": triggerBlink(spine); break;
    case "smile": triggerSmile(spine, face); break;
  }
}

/// Wall-clock time the channel stays busy for this action. For ear/tail this
/// includes the fade-back so the channel only frees once the body has settled.
export function actionDuration(kind: ActionKind, face: FaceState | null): number {
  switch (kind) {
    case "ear": return EAR_SECONDS + IDLE_FADE;
    case "tail": return TAIL_SECONDS + IDLE_FADE;
    case "blink": return 0.2;
    case "smile": return face ? face.smileDuration : 1.5;
  }
}

/// Mid-action: when only IDLE_FADE remains, start the smooth return for ear/tail.
/// No-op for blink/smile (they don't touch the spine, nothing to fade back).
export function beginFadeOut(spine: any, kind: ActionKind) {
  if (kind === "ear") spine.state.setEmptyAnimation(TRACK.ear, IDLE_FADE);
  else if (kind === "tail") spine.state.setEmptyAnimation(TRACK.tail, IDLE_FADE);
}

/// End-of-action cleanup (smile: turn the mouth override off).
export function endAction(kind: ActionKind, face: FaceState | null) {
  if (kind === "smile") endSmileMouth(face);
}

// ── Random intervals (seconds, wall-clock) ──
export function nextBlinkDelay(): number {
  return 4 + Math.random() * 2; // ~5s human cadence
}
export function nextSmileDelay(): number {
  return 12 + Math.random() * 6; // 12-18s sparse warmth
}
/// ear/tail cadence: with a 50/50 pick each part shows ~every 10-16s.
export function nextSpineDelay(): number {
  return 5 + Math.random() * 3; // 5-8s between ear/tail one-shots
}
