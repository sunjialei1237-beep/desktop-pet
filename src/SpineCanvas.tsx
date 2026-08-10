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
      const PIXI = await import("pixi.js");
      // Importing pixi-spine registers its Assets loader parser (side effect) for
      // Spine 3.8 skeletons before PIXI.Assets.load runs below.
      const { Spine } = await import("pixi-spine");

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

      try {
        // pixi-spine's loader auto-resolves the matching liri.atlas (same basename)
        // and the texture it references (skeleton.png, see skeleton_structure.md).
        const res = await PIXI.Assets.load("/spine/liri/liri.json");
        if (destroyed) return;

        const spine = new Spine((res as any).spineData);
        spineRef.current = spine;
        app.stage.addChild(spine);

        // Scale to fit the canvas, centered. Spine has no `.anchor`; center via
        // its post-scale bounds (pixi-spine getBounds reflects the transform).
        const fit = Math.min(app.screen.width / spine.width, app.screen.height / spine.height) * 0.9;
        spine.scale.set(fit);
        const b = spine.getBounds(true);
        spine.x = (app.screen.width - b.width) / 2 - b.x;
        spine.y = (app.screen.height - b.height) / 2 - b.y;

        // Base life layer: breathing on track 0 (loop). Idle secondary tracks
        // (ear/hair/tail) and expression layer come with the driver push.
        spine.state.setAnimation(0, "body_breath", true);

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
        console.error("[Spine] model load failed:", err);
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
