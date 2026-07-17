import { useEffect, useRef } from "react";
import { BehaviorState } from "./animation/fsm";
import { AttentionState } from "./animation/attention";

// PixiJS + Live2D integration for the desktop pet rendering layer.
// The Cubism Core library is loaded via a script tag in index.html.
// pixi-live2d-display is imported dynamically to keep the initial bundle small.

export interface PointerXY {
  x: number;
  y: number;
}

interface Live2DCanvasProps {
  moodLabel: string;
  behavior: BehaviorState;
  attention: AttentionState;
  pointerRef: React.MutableRefObject<PointerXY>;
  isThinking: boolean;
  onHeadClick: () => void;
  onBodyClick: () => void;
}

// Map internal mood labels to Haru model expression names.
// The model defines f00..f07 in model3.json.
const MOOD_EXPRESSION_NAME: Record<string, string> = {
  happy: "f00",
  playful: "f01",
  calm: "f02",
  sad: "f03",
  worried: "f04",
  tired: "f05",
};

function moodToExpressionName(label: string): string {
  if (label === "\u5f00\u5fc3") return MOOD_EXPRESSION_NAME.happy;       // happy
  if (label === "\u8c03\u76ae") return MOOD_EXPRESSION_NAME.playful;    // playful
  if (label === "\u5e73\u9759") return MOOD_EXPRESSION_NAME.calm;       // calm
  if (label === "\u96be\u8fc7" || label === "\u7b2e\u96be") return MOOD_EXPRESSION_NAME.sad;
  if (label === "\u62c5\u5fc3") return MOOD_EXPRESSION_NAME.worried;    // worried
  if (label === "\u75b2\u60eb") return MOOD_EXPRESSION_NAME.tired;      // tired
  return MOOD_EXPRESSION_NAME.calm;
}

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

export function Live2DCanvas({ moodLabel, behavior, attention, pointerRef, isThinking, onHeadClick, onBodyClick }: Live2DCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const modelRef = useRef<any>(null);
  const lastBehaviorRef = useRef<BehaviorState>(BehaviorState.Idle);

  // FIX-A / FIX-B: mirror the latest props into a ref read each ticker frame,
  // so focus/motion react instantly without re-running the heavy load effect.
  const propsRef = useRef({ attention, behavior, isThinking, moodLabel });
  propsRef.current = { attention, behavior, isThinking, moodLabel };

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
        const model = await Live2DModel.from(modelUrl);
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
          const m = modelRef.current;
          const canvas = canvasRef.current;
          if (!m || !canvas) return;
          if (propsRef.current.attention === AttentionState.Ignored) {
            m.focus(app.screen.width / 2, app.screen.height / 2);
          } else {
            const rect = canvas.getBoundingClientRect();
            const p = pointerRef.current;
            m.focus(p.x - rect.left, p.y - rect.top);
          }
        };
        app.ticker.add(focusTickerFn);
        (app as any).__focusFn = focusTickerFn;
      } catch (err) {
        console.error("[Live2D] model load failed:", err);
      }
    })();

    return () => {
      destroyed = true;
      const app = appRef.current;
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
    if (!model) return;
    const name = moodToExpressionName(moodLabel);
    try {
      model.expression(name);
    } catch {
      // expression may not be ready yet
    }
  }, [moodLabel]);

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
