import { useEffect, useRef } from "react";
import { BehaviorState } from "./animation/fsm";
import { setupMix, setupIdleTracks, triggerBehavior, initFace, playAction, actionDuration, beginFadeOut, endAction, nextBlinkDelay, nextSmileDelay, nextSpineDelay, IDLE_FADE, fatigueLevel, applyEmotionFace } from "./animation/spineIntent";
import type { ActionKind } from "./animation/spineIntent";
import type { EmotionVector } from "./animation/emotionDriver";

// Spine (3.8) + PixiJS rendering layer for Liri. Replaces the Live2DCanvas
// placeholder once verified.
//
// Driver layer (this file + spineIntent.ts): a SINGLE SERIAL action channel
// fires one of blink/ear/tail/smile at a time over a continuous body_breath
// base (track0) — they never overlap. ear/tail are one-shots (never looped —
// every Liri idle keys the spine chain) AND breath-aligned (fire only at
// body_breath's loop boundary so the body is at setup, killing spine jumps);
// blink/smile key only eye slots, so they fire freely on their own timers. The
// FSM BehaviorState drives an extra expression (wink) on behavior change.
// Live2D Cubism-param translation is replaced; intent sources
// (FSM/circadian/EmotionVector) are reused. Emotion→expression-slot (Phase 3)
// + gaze (Phase 4) pending. Contract: docs/specs/liri/{skeleton_structure,
// animation_spec}.md.

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SpineCanvasProps {
  // Circadian animation-speed multiplier (circadian.ts speedModifier). Applied
  // via app.ticker.speed, which scales deltaMS feeding our manual spine.update
  // (Architecture Principle #10). Default 1.0 = real-time.
  speedModifier: number;
  // FSM BehaviorState → drives the expression track (blink/wink on change).
  behavior: BehaviorState;
  // Continuous emotion vector → drives half-open eye slots (fatigue). Phase 3.
  emotionVector: EmotionVector;
  onHeadClick: () => void;
  onBodyClick: () => void;
  // Loose bounding rect for gaze/click-through (mirrors Live2DCanvas semantics).
  onModelBounds?: (b: Rect) => void;
  // Tight bounding rect for click hit testing.
  onModelHitBounds?: (b: Rect) => void;
  // Fired when the Spine asset fails to load, so App falls back to Live2D
  // instead of leaving a blank canvas.
  onLoadError?: () => void;
}

export function SpineCanvas({ speedModifier, behavior, emotionVector, onHeadClick, onBodyClick, onModelBounds, onModelHitBounds, onLoadError }: SpineCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const spineRef = useRef<any>(null);
  // Mirror latest props into refs read each ticker frame / effect (avoids
  // re-running the heavy load effect on every prop change — same pattern as
  // Live2DCanvas).
  const speedRef = useRef(speedModifier);
  speedRef.current = speedModifier;
  const behaviorRef = useRef(behavior);
  behaviorRef.current = behavior;
  const emoRef = useRef(emotionVector);
  emoRef.current = emotionVector;
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
        const fit = Math.min(app.screen.width / b1.width, app.screen.height / b1.height) * 0.9;
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

        // Report bounding rects for click-through (loose + tight), mirroring
        // Live2DCanvas so App's hit testing keeps working unchanged.
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
          const PAD = 0.40;
          const TOP_BIAS = 0.15;
          onModelBounds?.({
            x: b.x - w * PAD,
            y: b.y - h * PAD - h * TOP_BIAS,
            width: w * (1 + 2 * PAD),
            height: h * (1 + 2 * PAD) + h * TOP_BIAS,
          });
        } catch {
          // getBounds unavailable -- App keeps fully interactive (safe default).
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
          // Phase 3: continuous emotion → half-open eye slots, AFTER update() so
          // we override the idles' slot timelines this frame. Suppressed while a
          // blink/smile one-shot owns the expr track (they switch eyes themselves).
          const suppressed = busy && (busyKind === "blink" || busyKind === "smile");
          applyEmotionFace(face, fatigueLevel(emoRef.current), suppressed);
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
        if (!destroyed) onLoadError?.();
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
