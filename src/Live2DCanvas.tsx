import { useEffect, useRef } from "react";
import { BehaviorState } from "./animation/fsm";
import { AttentionState } from "./animation/attention";
import { getBehaviorParams } from "./animation/behaviorDriver";
import { boostForTransientExpression, getEmotionParams, type EmotionVector } from "./animation/emotionDriver";

// PixiJS + Live2D integration for the desktop pet rendering layer.
// The Cubism Core library is loaded via a script tag in index.html.
// pixi-live2d-display is imported dynamically to keep the initial bundle small.

export interface PointerXY {
  x: number;
  y: number;
}

// Gaze Ignored return point + vision comfort center: model head position
// (canvas-local coords). Haru's head sits near the top 1/6 of the canvas,
// horizontally centered.
const HEAD_FOCUS = { x: 200, y: 0 };

interface Live2DCanvasProps {
  emotionVector: EmotionVector;
  transientExpression: string | null;
  behavior: BehaviorState;
  attention: AttentionState;
  pointerRef: React.MutableRefObject<PointerXY>;
  isThinking: boolean;
  // Circadian animation-speed multiplier (circadian.ts speedModifier). Scales the
  // PIXI ticker delta so the library's idle breathing/blink/motion/physics slow
  // down at night and speed up in the morning -- the pet visibly gets drowsy
  // after dark (Architecture Principle #10: liveliness). Default 1.0 = real-time.
  speedModifier: number;
  onHeadClick: () => void;
  onBodyClick: () => void;
  // Loose bounding rect (25% padding) for gaze/click-through; keeps head above
  // and feet inside so vision isn't yanked to center prematurely.
  onModelBounds?: (b: { x: number; y: number; width: number; height: number }) => void;
  // Tight bounding rect (10% inset) for click hit testing; hugs the sprite.
  onModelHitBounds?: (b: { x: number; y: number; width: number; height: number }) => void;
}

// NOTE: the discrete mood->expression map (f00..f05) was removed in favor of
// continuous parameter interpolation (emotionDriver.getEmotionParams). The
// baseline mood now drives eye/mouth/eye-form params every frame instead of
// switching expression files. The per-turn *transient* expression (strong
// emotion from a reply) still uses model.expression() via `transientExpression`.

// FIX-B: Haru only exposes Idle x3 + Tap x2 motions (see model3.json).
// Map FSM microbehaviors onto Tap motion + expression overlays.
function applyBehaviorToModel(model: any, behavior: BehaviorState) {
  try {
    switch (behavior) {
      case BehaviorState.Yawn:
      case BehaviorState.Sleeping:
        model.expression("f05"); // tired / drowsy
        break;
      case BehaviorState.Embarrassed:
        model.expression("f04"); // worried / flustered
        model.motion("Tap");
        break;
      case BehaviorState.LookAround:
      case BehaviorState.TiltHead:
      case BehaviorState.Stretch:
      case BehaviorState.Peek:
      case BehaviorState.Sway:
      case BehaviorState.Hum:
      case BehaviorState.Blink:
      case BehaviorState.Talking:
      case BehaviorState.Thinking:
        model.motion("Tap");
        break;
      case BehaviorState.Idle:
      case BehaviorState.Recovering:
      default:
        // Let the library's idle breathing run untouched.
        break;
    }
  } catch {
    // model may not be fully ready yet
  }
}

export function Live2DCanvas({ emotionVector, behavior, attention, pointerRef, isThinking, onHeadClick, onBodyClick, onModelBounds, onModelHitBounds, transientExpression, speedModifier = 1.0 }: Live2DCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const modelRef = useRef<any>(null);
  const lastBehaviorRef = useRef<BehaviorState>(BehaviorState.Idle);
  // Timestamp (performance.now) at which the current behavior became active.
  // Used by the per-frame behavior ticker to drive time-based param curves.
  const behaviorStartRef = useRef<number>(0);
  const lastBehaviorSeenRef = useRef<BehaviorState>(BehaviorState.Idle);

  // FIX-A / FIX-B: mirror the latest props into a ref read each ticker frame,
  // so focus/motion react instantly without re-running the heavy load effect.
  const propsRef = useRef({ attention, behavior, isThinking, emotionVector, transientExpression, speedModifier });
  propsRef.current = { attention, behavior, isThinking, emotionVector, transientExpression, speedModifier };

  useEffect(() => {
    let destroyed = false;

    (async () => {
      // Verify Cubism Core is loaded before attempting model creation.
      const w = window as any;
      if (!w.Live2DCubismCore) {
        console.error("[Live2D] Cubism Core runtime not found. Check index.html script tag + CSP wasm-unsafe-eval.");
        return;
      }

      const PIXI = await import("pixi.js");
      // Use cubism4 subpath to avoid requiring the Cubism 2 runtime (live2d.min.js).
      const { Live2DModel } = await import("pixi-live2d-display-lipsyncpatch/cubism4");

      // Register the PIXI Ticker so the model animates (breathing, blink, motion).
      Live2DModel.registerTicker(PIXI.Ticker);

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

      try {
        const modelUrl = "/live2d/models/haru/haru_greeter_t03.model3.json";
        // autoFocus defaults to true (lib _Automator ctor, cubism4.es.js:10149).
        // When enabled, the library binds globalpointermove -> model.focus(),
        // and model.focus normalizes via atan2 (cubism4.es.js:10495), coupling
        // x/y onto a unit circle. That suppresses the y target (cursor to the
        // right => cos~1 => sin~0 => targetY~0 => she never looks up/down) and
        // races our per-frame focusTickerFn for the focusController target.
        // Disabling it makes our ticker the sole writer with independent x/y.
        // autoHitTest stays at its default true so "hit" events still fire for
        // Head/Body clicks.
        const model = await Live2DModel.from(modelUrl, { autoFocus: false });
        if (destroyed) {
          model.destroy();
          return;
        }

        app.stage.addChild(model);

        // Scale model to fit within the canvas, centered.
        const scale = Math.min(app.screen.width / model.width, app.screen.height / model.height) * 0.85;
        model.scale.set(scale);
        model.anchor.set(0.5, 0.5);
        model.x = app.screen.width / 2;
        model.y = app.screen.height / 2;

       modelRef.current = model;

        // Report model bounding rect (canvas-local CSS px) for click-through.
        // PIXI getBounds is authoritative; fallback to geometric estimate from
        // the anchor-centered placement if it misbehaves. Apply a small inset
        // (10%) so the hit region hugs the sprite rather than transparent edges.
        try {
          const b = model.getBounds();
          const w = b.width;
          const h = b.height;
          // Tight (10% inset): click hit testing, hugs the sprite.
          const INSET = 0.10;
          onModelHitBounds?.({
            x: b.x + w * INSET,
            y: b.y + h * INSET,
            width: w * (1 - 2 * INSET),
            height: h * (1 - 2 * INSET),
          });
          // Loose bounds drive click-through. User: "人物本体 + 小圈". PAD=0.10
          // (was 0.40 — that made the hit rect cover most of the canvas and
          // blank-area clicks never reached the desktop). Keep in sync with
          // SpineCanvas.tsx. TOP_BIAS=0.05 keeps a little headroom.
          const PAD = 0.10;
          const TOP_BIAS = 0.05;
          onModelBounds?.({
            x: b.x - w * PAD,
            y: b.y - h * PAD - h * TOP_BIAS,
            width: w * (1 + 2 * PAD),
            height: h * (1 + 2 * PAD) + h * TOP_BIAS,
          });
        } catch {
          // getBounds unavailable — App side will keep fully interactive (safe default).
        }

        model.on("hit", (hitAreas: string[]) => {
          if (hitAreas.includes("Head")) {
            onHeadClick();
          } else if (hitAreas.includes("Body")) {
            onBodyClick();
          }
        });

        // FIX-A: per-frame focus override. The library auto-tracks the global
        // mouse and never lets the gaze return to center; our last-write-per-frame
        // wins, so Ignored forces the gaze back to the model's front.
        const focusTickerFn = () => {
          // Circadian animation speed: scale the ticker delta so idle breathing,
          // blink, motion and physics all slow down at night / perk up in the
          // morning (Architecture Principle #10). Set every frame from the latest
          // circadian speedModifier prop; 1.0 (afternoon) is PIXI's default.
          app.ticker.speed = propsRef.current.speedModifier;
          const m = modelRef.current;
          const canvas = canvasRef.current;
          if (!m || !canvas) return;
          let focusX: number;
          let focusY: number;
          if (propsRef.current.attention === AttentionState.Ignored) {
            focusX = HEAD_FOCUS.x;
            focusY = HEAD_FOCUS.y;
          } else {
            const rect = canvas.getBoundingClientRect();
            const p = pointerRef.current;
            focusX = p.x - rect.left;
            focusY = p.y - rect.top;
          }
          // Bypass model.focus(): it normalizes via atan2, coupling x/y onto a
          // unit circle so independent 360-degree tracking is impossible. Instead
          // drive internalModel.focusController directly with independent x/y in
          // [-1,1] (no atan2). focusX/focusY are canvas-local CSS px here.
         const im = (m as unknown as {
           internalModel: {
             focusController: { focus: (nx: number, ny: number, instant?: boolean) => void };
           };
         }).internalModel;
          // Normalize against the canvas size (the coordinate system focusX/focusY
          // actually live in), NOT the model's original Cubism canvas. Center
          // (200,300) -> (0,0) front; top -> ny=-1 (max look up); edges -> +/-1.
          // x and y are independent (no atan2 coupling), enabling true 360 tracking.
          const nx = Math.max(-1, Math.min(1, (focusX / app.screen.width) * 2 - 1));
          // Canvas-local y grows downward (cursor above => focusY small => raw
          // value ~ -1), but Cubism ParamAngleY positive = look up. Invert y so
          // ny is already in "look direction" space (cursor above => ny > 0 =>
          // she looks up), mirroring the lib's own -sin(radian) flip in
          // model.focus (cubism4.es.js:10496). x needs no flip.
          const ny = -Math.max(-1, Math.min(1, (focusY / app.screen.height) * 2 - 1));
          im.focusController.focus(nx, ny);
        };
        app.ticker.add(focusTickerFn);
        (app as any).__focusFn = focusTickerFn;

        // Behavior parameter driver. We hook the library's `beforeModelUpdate`
        // event: it fires each frame AFTER motion/focus/blink/physics/pose have
        // written their params but BEFORE coreModel.update()+loadParameters()
        // commit+render. Writing here makes our overlay the LAST writer that
        // actually reaches the render, so LookAround really turns the head,
        // Sway really sways, etc. The moment a behavior ends (Idle/Talking
        // return {}) we stop overriding and the library idle + focus resume.
        // We never touch focusController, so gaze tracking is unaffected.
        const im = (model as unknown as {
          internalModel: {
            coreModel: { setParameterValueById: (id: string, v: number) => void };
            on: (ev: string, fn: () => void) => void;
            off: (ev: string, fn: () => void) => void;
          };
        }).internalModel;
        const beforeModelUpdateFn = () => {
          const beh = propsRef.current.behavior;
          if (beh !== lastBehaviorSeenRef.current) {
            lastBehaviorSeenRef.current = beh;
            behaviorStartRef.current = performance.now();
          }
          const elapsed = performance.now() - behaviorStartRef.current;
          // Emotion baseline, layered UNDER the behavior overlay so active
          // microbehaviors (Sleeping/Blink/Yawn...) still win the params they
          // touch. When a transient per-turn expression is active, step back
          // ({}) and let the preset expression own the face -- otherwise our
          // continuous writes would overwrite its parameters every frame
          // (Architecture Principle #10: continuous emotion -> liveliness).
          // Transient per-turn emotion (strong signal from the reply) nudges
          // the SAME continuous vector instead of switching to a preset
          // expression file -- one unified path, no residual params when it
          // ends (Architecture Principle #10).
          const emoVector = propsRef.current.transientExpression
            ? boostForTransientExpression(propsRef.current.transientExpression, propsRef.current.emotionVector)
            : propsRef.current.emotionVector;
          const params = { ...getEmotionParams(emoVector), ...getBehaviorParams(beh, elapsed) };
          const core = im.coreModel;
          for (const id in params) {
            try {
              core.setParameterValueById(id, params[id]);
            } catch {
              // param id not present on this model -> skip silently
            }
          }
        };
        im.on("beforeModelUpdate", beforeModelUpdateFn);
        (model as any).__behaviorFn = beforeModelUpdateFn;
      } catch (err) {
        console.error("[Live2D] model load failed:", err);
      }
    })();

    return () => {
      destroyed = true;
      const app = appRef.current;
      if (modelRef.current && (modelRef.current as any).__behaviorFn) {
        // Detach the behavior overlay before destroying the model.
        try {
          (modelRef.current as any).internalModel?.off(
            "beforeModelUpdate",
            (modelRef.current as any).__behaviorFn,
          );
        } catch {
          // model may already be torn down
        }
      }
      if (app && (app as any).__focusFn) {
        app.ticker.remove((app as any).__focusFn);
      }
      if (modelRef.current) {
        modelRef.current.destroy();
        modelRef.current = null;
      }
      if (appRef.current) {
        appRef.current.destroy(true);
        appRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // FIX-B: react to FSM behavior changes by triggering the mapped motion/expression.
  useEffect(() => {
    const model = modelRef.current;
    if (!model) return;
    if (behavior === lastBehaviorRef.current) return;
    lastBehaviorRef.current = behavior;
    applyBehaviorToModel(model, behavior);
  }, [behavior]);

  useEffect(() => {
    const model = modelRef.current;
    if (!model || !isThinking) return;
    lastBehaviorRef.current = BehaviorState.Thinking;
    try {
      model.motion("Tap");
    } catch {
      // motion may not be ready
    }
  }, [isThinking]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "400px", height: "600px", display: "block" }}
    />
  );
}
