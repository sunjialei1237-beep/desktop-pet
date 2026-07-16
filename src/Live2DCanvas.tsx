import { useEffect, useRef } from "react";

// PixiJS + Live2D integration for the desktop pet rendering layer.
// This replaces the SVG PetCharacter with a proper Live2D Cubism 4 model.
// The Cubism Core library is loaded via a script tag in index.html.
// pixi-live2d-display is imported dynamically to keep the initial bundle small.

interface Live2DCanvasProps {
  moodLabel: string;
  isThinking: boolean;
  onHeadClick: () => void;
  onBodyClick: () => void;
}

// Map internal mood labels to Haru model expression indices (F01-F08).
const MOOD_EXPRESSION: Record<string, number> = {
  happy: 0,
  playful: 1,
  calm: 2,
  sad: 3,
  worried: 4,
  tired: 5,
};

function moodToIndex(label: string): number {
  if (label === "\u5f00\u5fc3") return MOOD_EXPRESSION.happy;
  if (label === "\u8c03\u76ae") return MOOD_EXPRESSION.playful;
  if (label === "\u5e73\u9759") return MOOD_EXPRESSION.calm;
  if (label === "\u96be\u8fc7" || label === "\u7b2e\u96be") return MOOD_EXPRESSION.sad;
  if (label === "\u62c5\u5fc3") return MOOD_EXPRESSION.worried;
  if (label === "\u75b2\u60eb") return MOOD_EXPRESSION.tired;
  return MOOD_EXPRESSION.calm;
}

export function Live2DCanvas({ moodLabel, isThinking, onHeadClick, onBodyClick }: Live2DCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const appRef = useRef<any>(null);
  const modelRef = useRef<any>(null);

  useEffect(() => {
    let destroyed = false;

    (async () => {
      const PIXI = await import("pixi.js");
      const { Live2DModel } = await import("pixi-live2d-display-lipsyncpatch");

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
        const model = await Live2DModel.from("/live2d/models/haru/haru_greeter_t03.model3.json");
        if (destroyed) {
          model.destroy();
          return;
        }

        app.stage.addChild(model);

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
      } catch (err) {
        console.error("Live2D model load failed:", err);
      }
    })();

    return () => {
      destroyed = true;
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

  useEffect(() => {
    const model = modelRef.current;
    if (!model) return;
    const idx = moodToIndex(moodLabel);
    try {
      model.expression(idx);
    } catch {
      // expression may not be ready yet
    }
  }, [moodLabel]);

  useEffect(() => {
    const model = modelRef.current;
    if (!model || !isThinking) return;
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
