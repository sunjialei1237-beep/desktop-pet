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

// ── Face: smile duration only ──
// ARCHITECTURE (user directive): 状态/情绪 → 播放对应动画；动画 timeline 自己
// 管 slot attachment（美术在 Spine 里做）。代码绝不 setAttachment 改 slot——之前
// 试过运行时手动切嘴/眼，破坏了美术 timeline，导致空眼/双层。FaceState 现在只
// 保留 smile 动画的时长（用于串行通道的 busy 计时），不再捕获任何 slot 引用。
export interface FaceState {
  smileDuration: number;
}

export function initFace(spine: any): FaceState | null {
  try {
    const sk = spine.skeleton;
    const smileAnim = sk.data.findAnimation("smile");

    // TEMP PATCH (remove after artist fixes setup pose): the 嘴/小笑嘴/张大笑嘴
    // slots' setup-pose attachments are set to "shown" in the asset, so they
    // render on top of 脸 (the full-face layer with closed mouth) and Liri's
    // mouth looks permanently open. body_breath t=0 nulls 张大笑嘴 every frame,
    // but NOTHING nulls 嘴/小笑嘴 — so we null them once at init. body_breath
    // re-applies its own nulls every frame, so a one-shot null here sticks.
    // Proper fix = artist sets these three slots' setup attachment to null in
    // Spine; then delete this block.
    try {
      for (const slotName of ["嘴", "小笑嘴", "张大笑嘴"]) {
        const slot = sk.findSlot(slotName);
        if (slot && slot.attachment) slot.setAttachment(null);
      }
    } catch {
      // best-effort: missing slot is fine, initFace still returns duration
    }

    return { smileDuration: smileAnim ? smileAnim.duration : 1.5 };
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
function triggerSmile(spine: any) {
  spine.state.setAnimation(TRACK.expr, "smile", false);
}

/// Start a one-shot action on its track.
export function playAction(spine: any, kind: ActionKind, _face: FaceState | null) {
  switch (kind) {
    case "ear": playEar(spine); break;
    case "tail": playTail(spine); break;
    case "blink": triggerBlink(spine); break;
    case "smile": triggerSmile(spine); break;
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

/// End-of-action cleanup. smile's mouth slot is owned by the animation timeline
/// (artist keys 嘴/张大笑嘴 attachment in the smile anim), so there is nothing
/// for code to restore — kept as a hook in case a future anim needs it.
export function endAction(_kind: ActionKind, _face: FaceState | null) {
  // intentionally empty: animations own their slot cleanup now
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
