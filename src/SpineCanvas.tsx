import { useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import { BehaviorState } from "./animation/fsm";
import {
  setupMix,
  setupIdleTracks,
  triggerBehavior,
  createProgramRunner,
  isProgramActive,
  tickProgram,
  breathBoundary,
  firePartAction,
  pickPartAction,
  nextBlinkDelay,
  nextSmileDelay,
  nextPartDelay,
  EXPR_DURATIONS,
  PART_ACTION_DURATION,
  IDLE_FADE,
  TRACK,
} from "./animation/spineIntent";
import type { RunnerState, PartAction } from "./animation/spineIntent";
import { patchLiriJson, isLiriSkeleton, LIRI_JSON_URL } from "./animation/liriAssetPatch";

// Spine (3.8) + PixiJS rendering layer for Liri (the sole renderer).
//
// Driver layer (this file + spineIntent.ts): calm-idle base = body_breath sway
// (+ subtle skirt/arm ambience) looping forever; ear/hair/tail fire as one-shot
// part actions at random ≥15s intervals; blink/smile run on facial timers.
// Emotion programs (sad/happy/curious/thing) are special cases driven by the
// runner: they start at a body_breath loop boundary and end n boundaries later
// when every member channel reverts in parallel (the breath-aligned rule).
// Contract: docs/specs/liri/{skeleton_structure, animation_spec}.md.

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

// --- Gaze (AIRI-style head-follow) tuning constants ---
// The head follows the cursor ONLY within GAZE_RANGE canvas px of the
// head bone, with a radial falloff (full effect at the head, zero at the
// range edge, smooth return to neutral beyond — AIRI's ignored-return).
// HORIZONTAL ONLY: in-plane rotation of the head (±GAZE_HEAD_H) + body lean
// (±GAZE_BODY), both pivoting at their bone origins (neck joint stays put —
// the chin must never translate, user: 下巴必须固定). A vertical channel
// (head nod) was removed: 2D in-plane bones cannot express pitch without
// translating the head, which detached it from the neck ("头飞起来了").
// Real up/down gaze needs a look_up/look_down artist animation later
// (state→animation per the Spine architecture decision).
// Injected INSIDE update()'s bake pipeline via the updateWorldTransform
// wrapper (post-update writes never render — pixi-spine bakes slots right
// after updateWorldTransform). apply() resets locals every frame → no
// accumulation.
const GAZE_RANGE = 320;      // canvas px radius around the head bone
const GAZE_HEAD_H = 10;      // max head tilt (deg), horizontal
const GAZE_BODY = 3;         // max body lean (deg)
const GAZE_TAU = 0.12;       // smoothing time constant (s, wall clock)
const GAZE_H_SIGN = -1;      // horizontal mirror (user: 方向是反的 → -1)

// Register a PIXI LoadParser that intercepts liri.json BEFORE pixi-spine parses
// it, applies the runtime mouth-slot patch (see liriAssetPatch.ts), and returns
// the patched object. Priority High so it beats the generic json loader (Low).
// Idempotent + guarded by isLiriSkeleton, so it's a no-op for any other JSON.
// MUST be called before PIXI.Assets.load(LIRI_JSON_URL). Kept module-scoped so
// the loader is registered exactly once per page lifetime.
let liriPatchRegistered = false;
function registerLiriPatch(PIXI: any) {
  if (liriPatchRegistered) return;
  liriPatchRegistered = true;
  PIXI.extensions.add({
    extension: { type: PIXI.ExtensionType.LoadParser, priority: PIXI.LoaderParserPriority.High },
    name: "liriMouthPatch",
    test(url: string) {
      return url === LIRI_JSON_URL || url.endsWith("/liri/liri.json");
    },
    async load(url: string) {
      const res = await PIXI.settings.ADAPTER.fetch(url);
      const json = await res.json();
      if (patchLiriJson(json)) {
        // One-time confirmation; useful until the artist fixes the asset.
        console.info("[Spine] liri.json mouth-slot patch applied");
      } else if (isLiriSkeleton(json)) {
        console.info("[Spine] liri.json already patched (no-op)");
      }
      return json;
    },
  });
}

export interface SpineCanvasProps {
  // Circadian animation-speed multiplier (circadian.ts speedModifier). Applied
  // via app.ticker.speed, which scales deltaMS feeding our manual spine.update
  // (Architecture Principle #10). Default 1.0 = real-time.
  speedModifier: number;
  // FSM BehaviorState → drives the expression track (blink/wink on change).
  behavior: BehaviorState;
  // Cursor position in window (client) coords, kept fresh by App's
  // global-cursor listener. Drives head-gaze + body lean. A ref (not state)
  // so gaze reads it per frame without React re-renders.
  pointerRef: MutableRefObject<{ x: number; y: number }>;
  onHeadClick: () => void;
  onBodyClick: () => void;
  // Loose bounding rect for gaze/click-through.
  onModelBounds?: (b: Rect) => void;
  // Tight bounding rect for click hit testing.
  onModelHitBounds?: (b: Rect) => void;
  // Visual body rect WITHOUT padding (the rendered pixels). Drives the drag
  // screen walls so the head/feet can touch the screen edges exactly.
  onVisualBounds?: (b: Rect) => void;
  // Live head/feet anchor (canvas-local CSS px), reported once after measure.
  // headX = head-bone origin x (≈ face center), headY = model top + 8 (≈
  // crown below the ear tips), feetY = model bottom. App converts to window
  // coords (+150 canvas top offset) and anchors the speech-bubble tail tip
  // and the input box to the REAL model pose — replacing the scale-0.7-era
  // hardcoded pixels that drifted when the fit factor changed to 0.5.
  onBodyAnchor?: (a: { headX: number; headY: number; feetY: number }) => void;
}

export function SpineCanvas({ speedModifier, behavior, pointerRef, onHeadClick, onBodyClick, onModelBounds, onModelHitBounds, onVisualBounds, onBodyAnchor }: SpineCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const spineRef = useRef<any>(null);
  // Program runner lives inside the load effect's closure; mirror it here so
  // the [behavior] effect can ask whether a program owns the face channel.
  const runnerRef = useRef<RunnerState | null>(null);
  // Mirror latest props into refs read each ticker frame / effect (avoids
  // re-running the heavy load effect on every prop change).
  const speedRef = useRef(speedModifier);
  speedRef.current = speedModifier;
  const behaviorRef = useRef(behavior);
  behaviorRef.current = behavior;
  const lastBehaviorRef = useRef<BehaviorState | null>(null);

  useEffect(() => {
    let destroyed = false;

    (async () => {
     try {
      const PIXI = await import("pixi.js");
      // Liri is a Spine 3.8.75 export. loader-uni auto-detects the skeleton
      // version; the Spine class must come from the matching 3.8 runtime -- the
      // umbrella `pixi-spine` default is the 4.x runtime, which rejects 3.8 data
      // ("3.8.75 is deprecated, export with a newer version of Spine").
      await import("@pixi-spine/loader-uni");
      // Intercept liri.json on load and apply the runtime mouth-slot patch
      // BEFORE pixi-spine parses it (see liriAssetPatch.ts). Must run before
      // PIXI.Assets.load below.
      registerLiriPatch(PIXI);
      const { Spine } = await import("@pixi-spine/runtime-3.8");

      if (destroyed || !canvasRef.current) return;

      const app = new PIXI.Application({
        backgroundAlpha: 0,
        resolution: window.devicePixelRatio || 1,
        autoDensity: true,
        width: 400,
        height: 600,
        antialias: true,
        view: canvasRef.current,
      });
      appRef.current = app;

      // Circadian speed: set the ticker delta scale every frame from the latest
      // speedModifier prop. 1.0 (afternoon) is PIXI's default; DeepNight is 0.4.
      const speedTickerFn = () => {
        app.ticker.speed = speedRef.current;
      };
      app.ticker.add(speedTickerFn);
      (app as any).__speedFn = speedTickerFn;

        // pixi-spine's loader auto-resolves the matching liri.atlas (same basename)
        // and the texture it references (skeleton.png, see skeleton_structure.md).
        const res = await PIXI.Assets.load("/spine/liri/liri.json");
        if (destroyed) return;

        const spine = new Spine((res as any).spineData);
        spineRef.current = spine;
        app.stage.addChild(spine);
        // Dev aid: expose the spine instance for CDP debugging.
        (window as any).__spine = spine;

        // Turn off pixi-spine's self-update. It drives update() via Date.now()
        // inside updateTransform, which BYPASSES PIXI's ticker — so circadian
        // app.ticker.speed never reached the skeleton (a latent bug: Spine Liri
        // ignored day/night speed). We drive update ourselves from the ticker
        // using deltaMS (which IS scaled by app.ticker.speed), fixing that and
        // giving a known post-update hook point for Phase 3 slot overrides.
        spine.autoUpdate = false;
        setupMix(spine.stateData);
        setupIdleTracks(spine); // ALL base loops breathe/skirt/hair/arm/ear/tail
        spine.update(0); // apply pose before measuring

        // Measure at scale=1. pixi-spine bakes mesh vertices into a cache at
        // update() time; a later scale.set() does NOT recompute them, so
        // getBounds() reports the unscaled size. Centering on that stale bounds
        // pushes Liri down until only her upper body is on screen. Measure at
        // scale 1, then do the scaled centering math ourselves.
        const b1 = spine.getBounds(true);
        // Scale factor:璃缩到刚好填满 canvas(取宽高较小者)再 ×系数。
        // 0.5 = 再小一号（用户 2026-08-28：0.7 → 0.5）。
        // 改这一个值即可——居中(spine.x/y)、穿透判定(onModelBounds)、
        // 边界框全部基于 fit 自动联动。
        const fit = Math.min(app.screen.width / b1.width, app.screen.height / b1.height) * 0.5;
        spine.scale.set(fit);
        spine.x = app.screen.width / 2 - (b1.x + b1.width / 2) * fit;
        spine.y = app.screen.height / 2 - (b1.y + b1.height / 2) * fit;
        // On-screen bounds for click hit-testing (getBounds lies post-scale, so
        // derive the world rectangle from the scale-1 bounds manually).
        const b = {
          x: spine.x + b1.x * fit,
          y: spine.y + b1.y * fit,
          width: b1.width * fit,
          height: b1.height * fit,
        };

        // Report bounding rects for click-through (loose + tight).
        try {
          // Visual rect first (no padding) — the drag walls hug the body.
          onVisualBounds?.(b);
          const w = b.width;
          const h = b.height;
          const INSET = 0.10;
          onModelHitBounds?.({
            x: b.x + w * INSET,
            y: b.y + h * INSET,
            width: w * (1 - 2 * INSET),
            height: h * (1 - 2 * INSET),
          });
          // Loose bounds drive click-through (transparent regions forward clicks
          // to the desktop). User: "人物本体 + 小圈可交互,其余穿透". PAD=0.10
          // gives a small 10% margin around the model so the edges are still
          // draggable but blank area passes through. (Was 0.40, which made the
          // hit rect cover most of the 400×600 canvas → clicks on blank area
          // never reached the desktop.) TOP_BIAS=0.05 keeps a little headroom.
          const PAD = 0.10;
          const TOP_BIAS = 0.05;
          onModelBounds?.({
            x: b.x - w * PAD,
            y: b.y - h * PAD - h * TOP_BIAS,
            width: w * (1 + 2 * PAD),
            height: h * (1 + 2 * PAD) + h * TOP_BIAS,
          });
          // Head/feet anchor for the bubble tail tip + input box (see prop doc).
          // feetY uses the FOOT BONES, not bounds: getBounds' bottom is the
          // setup-pose skeleton extent — the tail chain hangs to canvas bottom
          // there — so bounds bottom sits ~60px (at fit 0.5) BELOW her soles,
          // which pushed the input box too low (用户 2026-08-28). Ankle bone +
          // 40 model-units of shoe ≈ sole. Sanity-clamped to the lower half of
          // the bounds; falls back to bounds bottom if the bones are missing
          // or the convention ever changes under us.
          const headBoneForAnchor = spine.skeleton.findBone("head");
          const footL = spine.skeleton.findBone("foot_L");
          const footR = spine.skeleton.findBone("foot_R");
          let feetY = b.y + b.height;
          const ankleY = Math.max(footL?.worldY ?? -Infinity, footR?.worldY ?? -Infinity);
          if (Number.isFinite(ankleY)) {
            const boneFeet = spine.y + ankleY * spine.scale.y + 40 * fit;
            if (boneFeet > b.y + b.height * 0.5 && boneFeet <= b.y + b.height) feetY = boneFeet;
          }
          onBodyAnchor?.({
            headX: headBoneForAnchor
              ? spine.x + headBoneForAnchor.worldX * spine.scale.x
              : b.x + b.width / 2,
            headY: b.y + 8, // model top = ear tips; +8 lands at the crown
            feetY,
          });
          console.log(
            "[anchor] head/feet reported",
            Math.round(feetY),
            Number.isFinite(ankleY) && feetY !== b.y + b.height ? "(foot bone)" : "(bounds fallback)",
          );
        } catch (e) {
          // getBounds unavailable -- App keeps fully interactive (safe default).
          // Log so a silent throw (the click-through "never reports bounds"
          // failure mode) is visible instead of swallowed.
          console.warn("[Spine] bounds report failed", e);
        }

        // Drive the skeleton ourselves (autoUpdate is off). Two clocks:
        //  - dt   = deltaMS/1000, scaled by app.ticker.speed → feeds spine.update,
        //           so animation PLAYBACK slows at night (circadian, Principle #10).
        //  - wall = elapsedMS/1000, real wall-clock → drives event INTERVALS, so
        //           "how often" is stable day or night (scaling it once made the
        //           user see ~1min gaps). NOTE: elapsedMS is a PER-FRAME delta,
        //           NOT a clock — countdowns subtract it every frame; never
        //           store "wall + duration" as a timestamp (that bug froze the
        //           whole scheduler after the first blink, 续⁶⁵).
        //
        // CALM-IDLE MODEL (user 2026-08-28): base = breath + skirt/arm ambience.
        // Ear/hair/tail fire as ONE-SHOT part actions at random ≥15s intervals.
        // Blink/smile run on their own facial timers. Emotion programs are
        // special cases, started/ended on breath boundaries via the runner;
        // while one runs, all idle timers hold. Expressions stay serial via a
        // countdown so a later setAnimation can't cut a playing smile in half.
        const runner: RunnerState = createProgramRunner();
        runnerRef.current = runner;
        let blinkT = nextBlinkDelay(); // ~5s
        let smileT = nextSmileDelay(); // 12-18s
        let partT = nextPartDelay(); // ≥15s between ear/hair/tail actions
        let exprBusyRem = 0; // countdown: expr queue busy while > 0
        let partRem = 0; // countdown: current part action remaining (incl. fade)
        let partFaded = false; // setEmptyAnimation already issued for the part action
        let partTrack: number = TRACK.ear; // which track the current part action runs on

        // Gaze state: smoothed head/body rotation (deg). Applied INSIDE the
        // update() bake pipeline via the updateWorldTransform wrapper below —
        // pixi-spine bakes slot transforms / mesh vertices right after
        // skeleton.updateWorldTransform(), so a rotation written after update()
        // returns never reaches the renderer (stale bake, the "no visual
        // effect" bug — same family as the 续¹² stale-bounds / 续¹⁸ sprite
        // cache traps). The wrapper adds the gaze offset to the head/spine
        // LOCALS between two world passes, so the bake that follows uses
        // gaze-inclusive matrices. apply() resets locals every frame → no
        // accumulation.
        const headBone = spine.skeleton.findBone("head");
        const bodyBone = spine.skeleton.findBone("spine");
        let gazeHead = 0;   // smoothed head tilt (deg)
        let gazeBody = 0;   // smoothed body lean (deg)
        const origUWT = spine.skeleton.updateWorldTransform.bind(spine.skeleton);
        spine.skeleton.updateWorldTransform = () => {
          origUWT();
          if (headBone && bodyBone) {
            // ADDITIVE rotation only: idle animations key head/spine
            // ROTATION every frame (apply() resets the base each frame), and
            // rotation pivots at the bone origin — the neck/chin joint never
            // translates. No translation channel: it detached the head from
            // the neck (user: 下巴必须固定).
            headBone.rotation += gazeHead;
            bodyBone.rotation += gazeBody;
          }
          origUWT();
        };
        // Live diagnostics for CDP debugging (mirrors __updateFn pattern).
        const gazeDiag = { head: 0, body: 0, dist: 0, f: 0, cx: 0, cy: 0, hx: 0, hy: 0 };
        (app as any).__gazeDiag = gazeDiag;
        (window as any).__gazeDiag = gazeDiag;

        // body_breath (track0) completes once per loop — the only moment the
        // body is guaranteed back at its start pose. One sync point closes the
        // active program (all members revert in parallel) and starts a queued
        // one, keeping the 「动作在呼吸起点并行结束」 contract exactly.
        const onBreathComplete = (entry: any) => {
          if (entry.trackIndex === 0) breathBoundary(spine, runner);
        };
        spine.state.addListener({ complete: onBreathComplete });

        const updateFn = () => {
          const dt = app.ticker.deltaMS / 1000;
          const wall = app.ticker.elapsedMS / 1000;

          // --- Gaze smoothing: compute targets BEFORE spine.update(). The
          // rotation is injected into the bake pipeline by the
          // updateWorldTransform wrapper (see above). Head anchor comes from
          // last frame's worlds — one frame of lag, imperceptible. Wall-clock
          // frame delta: elapsedMS is the PER-FRAME elapsed ms (like the
          // action timers), unaffected by ticker.speed — gaze responsiveness
          // never slows with circadian speed. (Do NOT subtract consecutive
          // elapsedMS values: it is already a delta, the difference is ~0 and
          // the smoothing freezes.)
          {
            const wallDt = app.ticker.elapsedMS / 1000;
            const canvas = canvasRef.current;
            if (canvas && headBone && bodyBone) {
              const rect = canvas.getBoundingClientRect();
              const cx = pointerRef.current.x - rect.left;
              const cy = pointerRef.current.y - rect.top;
              // Head bone world pos → canvas coords (spine.x/y + local*fit).
              const hx = spine.x + headBone.worldX * spine.scale.x;
              const hy = spine.y + headBone.worldY * spine.scale.y;
              const dx = cx - hx;
              const dy = cy - hy;
              const dist = Math.hypot(dx, dy);
              // Sleeping → she doesn't follow; out of range → smooth return.
              const active =
                behaviorRef.current !== BehaviorState.Sleeping && dist < GAZE_RANGE;
              const f = active ? 1 - dist / GAZE_RANGE : 0; // radial falloff
              const nx = dx / GAZE_RANGE;
              const targetHead = f * (nx * GAZE_HEAD_H * GAZE_H_SIGN);
              const targetBody = f * (nx * GAZE_BODY * GAZE_H_SIGN);
              const k = wallDt > 0 ? Math.min(1, wallDt / GAZE_TAU) : 0;
              gazeHead += (targetHead - gazeHead) * k;
              gazeBody += (targetBody - gazeBody) * k;
              gazeDiag.head = gazeHead;
              gazeDiag.body = gazeBody;
              gazeDiag.dist = dist;
              gazeDiag.f = f;
              gazeDiag.cx = cx;
              gazeDiag.cy = cy;
              gazeDiag.hx = hx;
              gazeDiag.hy = hy;
            }
          }

          spine.update(dt);

          // Advance the active program (mid-window smile repeats, sustained
          // sad face re-triggers). No-op when nothing is running.
          tickProgram(spine, runner, runner.activeDef, wall);
          if (isProgramActive(runner)) {
            return; // a program owns every channel; idle timers hold
          }

          // Run down the expression queue and the part action. Counters are
          // decremented by the per-frame delta (see the two-clocks note above).
          let exprBusy = false;
          if (exprBusyRem > 0 && (exprBusyRem -= wall) > 0) exprBusy = true;
          let partBusy = false;
          if (partRem > 0) {
            partRem -= wall;
            if (!partFaded && partRem <= IDLE_FADE) {
              // Fade the part track out so the body settles back to base.
              spine.state.setEmptyAnimation(partTrack, IDLE_FADE);
              partFaded = true;
            }
            if (partRem > 0) partBusy = true;
            else partRem = 0;
          }

          // Facial timers (blink/smile): independent of part actions — the
          // expr track is serial only against itself.
          if (!exprBusy) {
            if ((blinkT -= wall) <= 0) {
              spine.state.setAnimation(TRACK.expr, "blink", false);
              exprBusyRem = EXPR_DURATIONS.blink;
              blinkT = nextBlinkDelay();
            } else if ((smileT -= wall) <= 0) {
              spine.state.setAnimation(TRACK.expr, "smile", false);
              exprBusyRem = EXPR_DURATIONS.smile;
              smileT = nextSmileDelay();
            }
          }

          // Sporadic part action: ear/hair/tail one-shot, ≥15s apart.
          if (!partBusy && (partT -= wall) <= 0) {
            const part: PartAction = pickPartAction();
            partTrack = TRACK[part];
            firePartAction(spine, part);
            partRem = PART_ACTION_DURATION[part] + IDLE_FADE;
            partFaded = false;
            partT = nextPartDelay();
          }
        };
        app.ticker.add(updateFn);
        (app as any).__updateFn = updateFn;

        // Seed expression for the behavior already active at load — the
        // [behavior] effect below may have run before the spine finished
        // loading (it no-ops while spineRef is null).
        triggerBehavior(spine, behaviorRef.current, false);
        lastBehaviorRef.current = behaviorRef.current;

        // Click hit testing: Liri has no Spine hit boxes wired yet, so map by a
        // vertical split (upper 55% = head, lower = body). Placeholder until
        // real polygon hit areas land.
        const handleClick = (ev: MouseEvent) => {
          const rect = canvasRef.current!.getBoundingClientRect();
          const ry = (ev.clientY - rect.top) / rect.height;
          if (ry < 0.55) onHeadClick();
          else onBodyClick();
        };
        canvasRef.current.addEventListener("click", handleClick);
        (app as any).__clickFn = handleClick;
      } catch (err) {
        console.error("[Spine] init/load failed:", err);
      }
    })();

    return () => {
      destroyed = true;
      const app = appRef.current;
      const canvas = canvasRef.current;
      if (app && (app as any).__clickFn && canvas) {
        canvas.removeEventListener("click", (app as any).__clickFn);
      }
      if (app && (app as any).__speedFn) {
        app.ticker.remove((app as any).__speedFn);
      }
      if (app && (app as any).__updateFn) {
        app.ticker.remove((app as any).__updateFn);
      }
      if (app) {
        app.destroy(true);
        appRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Behavior → expression track. No-ops until the spine is loaded (the load
  // effect seeds the initial value once the spine exists), and skips repeats.
  // Winks are also suppressed while a composite program owns the face channel.
  useEffect(() => {
    const spine = spineRef.current;
    if (!spine) return;
    if (lastBehaviorRef.current === behavior) return;
    lastBehaviorRef.current = behavior;
    const running = runnerRef.current ? isProgramActive(runnerRef.current) : false;
    triggerBehavior(spine, behavior, running);
  }, [behavior]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "400px", height: "600px", display: "block" }}
    />
  );
}
