import { useEffect, useRef } from "react";

// Spine (3.8) + PixiJS rendering layer for Liri. Replaces the Live2DCanvas
// placeholder once verified. Rendered behind the `?spine=1` URL flag in App.tsx
// so the working Live2D path stays as fallback during migration.
//
// MVP scope (this file): load liri.json/atlas/png, display the skeleton centered,
// play `body_breath` on loop, apply the circadian speedModifier. The full driver
// layer (layered idle tracks, expression slot switching, gaze, FSM/emotion
// mapping, test panel) lands in the next push -- see
// docs/specs/liri/{skeleton_structure,animation_spec}.md for the contract.

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SpineCanvasProps {
  // Circadian animation-speed multiplier (circadian.ts speedModifier). Scales
  // the PIXI ticker delta so breathing slows at night / perks up in the morning
  // (Architecture Principle #10). Default 1.0 = real-time.
  speedModifier: number;
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

export function SpineCanvas({ speedModifier, onHeadClick, onBodyClick, onModelBounds, onModelHitBounds, onLoadError }: SpineCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const spineRef = useRef<any>(null);
  // Mirror latest props into a ref read each ticker frame (avoids re-running the
  // heavy load effect on every prop change -- same pattern as Live2DCanvas).
  const speedRef = useRef(speedModifier);
  speedRef.current = speedModifier;

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

        // Apply the idle pose before measuring. A freshly-built Spine hasn't
        // run a world-transform update, so its bounds are stale.
        spine.state.setAnimation(0, "body_breath", true); // track 0 = base life (breath loop)
        spine.update(0);

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

        // Click hit testing: Liri has no Spine hit boxes wired yet, so map by a
        // vertical split (upper 55% = head, lower = body). Placeholder until the
        // driver push adds real polygon hit areas.
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
      if (app) {
        app.destroy(true);
        appRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "400px", height: "600px", display: "block" }}
    />
  );
}
