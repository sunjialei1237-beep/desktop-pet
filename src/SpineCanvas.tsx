import { useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import { BehaviorState } from "./animation/fsm";
import { setupMix, setupIdleTracks, triggerBehavior, initFace, playAction, actionDuration, beginFadeOut, endAction, nextBlinkDelay, nextSmileDelay, nextSpineDelay, IDLE_FADE } from "./animation/spineIntent";
import type { ActionKind } from "./animation/spineIntent";
import { patchLiriJson, isLiriSkeleton, LIRI_JSON_URL } from "./animation/liriAssetPatch";

// Spine (3.8) + PixiJS rendering layer for Liri (the sole renderer).
//
// Driver layer (this file + spineIntent.ts): a SINGLE SERIAL action channel
// fires one of blink/ear/tail/smile at a time over a continuous body_breath
// base (track0) — they never overlap. ear/tail are one-shots (never looped —
// every Liri idle keys the spine chain) AND breath-aligned (fire only at
// body_breath's loop boundary so the body is at setup, killing spine jumps);
// blink/smile key only eye slots, so they fire freely on their own timers. The
// FSM BehaviorState drives an extra expression (wink) on behavior change.
// Contract: docs/specs/liri/{skeleton_structure, animation_spec}.md.

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

// --- Gaze (AIRI-style head-follow) tuning constants ---
// The head turns toward the cursor ONLY within GAZE_RANGE canvas px of the
// head bone, with a radial falloff (full effect at the head, zero at the
// range edge, smooth return to neutral beyond — AIRI's ignored-return).
// Amplitudes are deliberately small (user: 幅度都不用太大): head ±8°h/±3.5°v,
// body (spine bone) leans ±2.5° — 身体微侧. Applied ADDITIVELY on top of
// whatever the animations set each frame (post-update), so body_breath /
// ear / tail keep playing underneath and nothing accumulates.
const GAZE_RANGE = 320;      // canvas px radius around the head bone
const GAZE_HEAD_H = 8;       // max head rotation (deg), horizontal
const GAZE_HEAD_V = 3.5;     // max head rotation (deg), vertical
const GAZE_BODY = 2.5;       // max body lean (deg)
const GAZE_TAU = 0.12;       // smoothing time constant (s, wall clock)
const GAZE_H_SIGN = 1;       // flip to -1 if left/right feels mirrored

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
}

export function SpineCanvas({ speedModifier, behavior, pointerRef, onHeadClick, onBodyClick, onModelBounds, onModelHitBounds }: SpineCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const spineRef = useRef<any>(null);
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

        // Turn off pixi-spine's self-update. It drives update() via Date.now()
        // inside updateTransform, which BYPASSES PIXI's ticker — so circadian
        // app.ticker.speed never reached the skeleton (a latent bug: Spine Liri
        // ignored day/night speed). We drive update ourselves from the ticker
        // using deltaMS (which IS scaled by app.ticker.speed), fixing that and
        // giving a known post-update hook point for Phase 3 slot overrides.
        spine.autoUpdate = false;
        setupMix(spine.stateData);
        setupIdleTracks(spine); // track0 body_breath only; ear/tail fire as one-shots
        spine.update(0); // apply pose before measuring

        // Measure at scale=1. pixi-spine bakes mesh vertices into a cache at
        // update() time; a later scale.set() does NOT recompute them, so
        // getBounds() reports the unscaled size. Centering on that stale bounds
        // pushes Liri down until only her upper body is on screen. Measure at
        // scale 1, then do the scaled centering math ourselves.
        const b1 = spine.getBounds(true);
        // Scale factor:璃缩到刚好填满 canvas(取宽高较小者)再 ×系数。
        // 0.7 = 缩到约原来的 78%(0.7/0.9),璃精致居中、留更多空白。
        // 改这一个值即可——居中(spine.x/y)、穿透判定(onModelBounds)、
        // 边界框全部基于 fit 自动联动。
        const fit = Math.min(app.screen.width / b1.width, app.screen.height / b1.height) * 0.7;
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
        //           user see ~1min gaps).
        //
        // SINGLE SERIAL ACTION CHANNEL: blink/ear/tail/smile fire ONE at a time
        // behind a shared busy flag — they never overlap (user: "做完才下一个",
        // "不要同时"). Within that:
        //  - blink/smile key only eye SLOTS, never the spine → no jump; they fire
        //    on their own independent wall-clock timers when the channel is free.
        //  - ear/tail key the SPINE chain → firing mid-breath makes the body jump
        //    from the breath's mid-cycle pose to the idle's first frame. So they
        //    fire ONLY at body_breath's loop boundary (each `complete`), where the
        //    body is back at setup and the idle's first frame (also setup) matches
        //    — zero jump. spinePending arms the fire; the breath completes it.
        const face = initFace(spine);
        let busy = false; // channel occupied by the current action
        let busyRem = 0; // wall-clock remaining for the current action
        let busyKind: ActionKind | null = null;
        let faded = false; // ear/tail: setEmptyAnimation already issued for this action
        let blinkT = nextBlinkDelay(); // ~5s
        let smileT = nextSmileDelay(); // 12-18s
        let spineT = nextSpineDelay(); // 5-8s → ear/tail each ~every 10-16s
        let spinePending = false; // spineT elapsed; wait for a breath boundary to fire

        // Gaze state: smoothed head/body rotation applied additively after
        // spine.update() each frame (never accumulates — update() resets the
        // bones to the animation's value first).
        const headBone = spine.skeleton.findBone("head");
        const bodyBone = spine.skeleton.findBone("spine");
        let gazeHead = 0; // current smoothed head rotation (deg)
        let gazeBody = 0; // current smoothed body lean (deg)
        // Live diagnostics for CDP debugging (mirrors __updateFn pattern).
        const gazeDiag = { head: 0, body: 0, dist: 0, f: 0, cx: 0, cy: 0, hx: 0, hy: 0 };
        (app as any).__gazeDiag = gazeDiag;
        (window as any).__gazeDiag = gazeDiag;

        const fireSpineAction = () => {
          const k: ActionKind = Math.random() < 0.5 ? "ear" : "tail";
          playAction(spine, k, face);
          busy = true;
          busyKind = k;
          busyRem = actionDuration(k, face);
          faded = false;
          spineT = nextSpineDelay();
        };

        // body_breath (track0) completes once per loop — the only moment the body
        // is guaranteed back at setup. Fire a pending ear/tail here.
        const onBreathComplete = (entry: any) => {
          if (entry.trackIndex === 0 && spinePending && !busy) {
            spinePending = false;
            fireSpineAction();
          }
        };
        spine.state.addListener({ complete: onBreathComplete });

        const updateFn = () => {
          const dt = app.ticker.deltaMS / 1000;
          const wall = app.ticker.elapsedMS / 1000;
          spine.update(dt);

          // --- Gaze: head follows cursor within range (AIRI-style) ---
          // Wall-clock frame delta: app.ticker.elapsedMS is the PER-FRAME
          // elapsed ms (like the action timers above), unaffected by
          // ticker.speed — gaze responsiveness never slows with circadian
          // speed. (Do NOT subtract consecutive elapsedMS values: it is
          // already a delta, the difference is ~0 and the smoothing freezes.)
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
              const ny = dy / GAZE_RANGE;
              const targetHead = f * (nx * GAZE_HEAD_H * GAZE_H_SIGN + ny * GAZE_HEAD_V);
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
              // Additive: keeps the animations' own head/spine motion
              // (body_breath keys them every frame) with gaze on top.
              headBone.rotation += gazeHead;
              bodyBone.rotation += gazeBody;
            }
          }

          if (busy) {
            busyRem -= wall;
            if (!faded && busyRem <= IDLE_FADE) {
              beginFadeOut(spine, busyKind!);
              faded = true;
            }
            if (busyRem <= 0) {
              endAction(busyKind!, face);
              busy = false;
              busyKind = null;
            }
            return; // channel busy: freeze the independent timers until it frees
          }
          // Channel free — advance independent timers, fire the first to elapse.
          if ((blinkT -= wall) <= 0) {
            playAction(spine, "blink", face);
            busy = true; busyKind = "blink"; busyRem = actionDuration("blink", face); faded = true;
            blinkT = nextBlinkDelay();
          } else if ((smileT -= wall) <= 0) {
            playAction(spine, "smile", face);
            busy = true; busyKind = "smile"; busyRem = actionDuration("smile", face); faded = true;
            smileT = nextSmileDelay();
          } else if ((spineT -= wall) <= 0) {
            spinePending = true; // ear/tail wait for the next breath boundary
          }
        };
        app.ticker.add(updateFn);
        (app as any).__updateFn = updateFn;

        // Seed expression for the behavior already active at load — the
        // [behavior] effect below may have run before the spine finished
        // loading (it no-ops while spineRef is null).
        triggerBehavior(spine, behaviorRef.current);
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
  useEffect(() => {
    const spine = spineRef.current;
    if (!spine) return;
    if (lastBehaviorRef.current === behavior) return;
    lastBehaviorRef.current = behavior;
    triggerBehavior(spine, behavior);
  }, [behavior]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "400px", height: "600px", display: "block" }}
    />
  );
}
