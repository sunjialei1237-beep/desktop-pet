import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { Live2DCanvas } from "./Live2DCanvas";
import { SettingsPanel } from "./SettingsPanel";
import { DebugPanel } from "./DebugPanel";
import { ContextMenu } from "./ContextMenu";
import { AnimationFSM, BehaviorState } from "./animation/fsm";
import { pickNextBehavior } from "./animation/microBehavior";
import { AttentionState, computeAttention, type PetRect } from "./animation/attention";
import { type PetPosition } from "./animation/physics";
import { SpatialMemory } from "./animation/spatial";
import { getCircadianState, deepNightMessages, TimeOfDay } from "./animation/circadian";

interface EmotionData {
  mood: number;
  mood_label: string;
  physical_energy: number;
  social_battery: number;
  stress: number;
  loneliness: number;
}

interface ProactiveAction {
  event_id: string | null;
  action_type: string;
  message_hint: string;
}

// 首次见面访谈问题（顺序即提问顺序）。答案存入 app_config，注入 system prompt 的 [Persona]。
const ONBOARD_QUESTIONS = [
  { key: "user_nickname", ask: "初次见面！我该怎么称呼你呀？" },
  { key: "personality_style", ask: "你希望我是什么性格呢？（温柔治愈 / 活泼调皮 / 知性冷静 / 毒舌……）" },
  { key: "relationship_style", ask: "我们是什么关系好呢？（伙伴 / 恋人 / 妹妹 / 助手……）" },
  { key: "pet_name", ask: "最后，给我起个名字吧？（想让我自己起，就回“你来想”）" },
] as const;

// Map mood label to bubble CSS class for emotion-driven styling (Design doc 6.3)
function bubbleClassForMood(label: string): string {
  if (label === "开心") return "bubble-happy";
  if (label === "调皮") return "bubble-playful";
  if (label === "难过") return "bubble-sad";
  if (label === "担心") return "bubble-worried";
  if (label === "疲惫") return "bubble-sad";
  return "bubble-calm";
}

export default function App() {
// 方案 B: window dimensions match tauri.conf.json. petPos = window top-left.
const WINDOW_W = 400;
const WINDOW_H = 760;

  const [bubbleText, setBubbleText] = useState("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  const [bubbleStyle, setBubbleStyle] = useState("");
  const [bubblePos, setBubblePos] = useState("");
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  const [isThinking, setIsThinking] = useState(false);
  const [transientExpression, setTransientExpression] = useState<string | null>(null);
  const [moodLabel, setMoodLabel] = useState("平静");
  const [showSettings, setShowSettings] = useState(false);
  const [showDebug, setShowDebug] = useState(false);
 const [attention, setAttention] = useState(AttentionState.Ignored);
 const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
const [awayMode, setAwayMode] = useState(false);
  const [onboarding, setOnboarding] = useState<{
    active: boolean;
    step: number;
    answers: Record<string, string>;
  } | null>(null);
  // Ref mirror so bubble listeners (captured once) can read the latest value
  // and suppress welcome bubbles during the interview.
  const onboardingActiveRef = useRef(false);
const bubbleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
const transientTimerRef = useRef<number | null>(null);
  const welcomeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [behavior, setBehavior] = useState<BehaviorState>(BehaviorState.Idle);
  const fsmRef = useRef<AnimationFSM | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const pokeCountRef = useRef(0);
 const pokeResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingPokeRef = useRef<ReturnType<typeof setTimeout> | null>(null);
 const lastPetTimeRef = useRef(0);
  const closenessRef = useRef(0);
  const moodRef = useRef(0.5);
  const energyRef = useRef(0.7);
 const spatialRef = useRef<SpatialMemory | null>(null);
  const [petPos, setPetPos] = useState<PetPosition | null>(null);
 const [isWalking, setIsWalking] = useState(false);
 const [isBeingDragged, setIsBeingDragged] = useState(false);
 const circadianRef = useRef(getCircadianState());
 const pointerRef = useRef({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
 const thinkStartRef = useRef(0);
  // Click-through state (ADR Phase 2).
  const ignoreRef = useRef(false);
  const windowOriginRef = useRef<{ x: number; y: number } | null>(null); // physical px
  const scaleFactorRef = useRef<number | null>(null);
  const modelBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const modelHitBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const canvasRectRef = useRef<{ left: number; top: number } | null>(null);

 if (!spatialRef.current) {
    spatialRef.current = new SpatialMemory();
  }

  if (!fsmRef.current) {
    fsmRef.current = new AnimationFSM();
    fsmRef.current.onStateChange((s) => setBehavior(s));
  }

 const showBubble = useCallback((text: string, duration = 8000, style = "", pos = "") => {
   setBubbleText(text);
   setBubbleStyle(style);
   setBubblePos(pos);
   setBubbleVisible(true);
   if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
  bubbleTimerRef.current = setTimeout(() => setBubbleVisible(false), duration);
}, []);

  // 启动首次见面访谈：问第一题 + 显示输入框。访谈期间屏蔽其它气泡，避免 welcome 覆盖第一题。
  const startOnboarding = useCallback(() => {
    onboardingActiveRef.current = true;
    if (welcomeTimerRef.current) {
      clearTimeout(welcomeTimerRef.current);
      welcomeTimerRef.current = null;
    }
    setOnboarding({ active: true, step: 0, answers: {} });
    showBubble(ONBOARD_QUESTIONS[0].ask, 120000, "bubble-calm");
    setInputVisible(true);
  }, [showBubble]);

  // Receive model bounds (canvas-local CSS px) from Live2DCanvas for click-through.
  const handleModelBounds = useCallback((b: { x: number; y: number; width: number; height: number }) => {
    modelBoundsRef.current = b;
    // Capture the canvas wrapper rect (viewport-relative CSS px); the canvas fills it.
    const el = petRef.current;
    if (el) {
      const r = el.getBoundingClientRect();
      canvasRectRef.current = { left: r.left, top: r.top };
    }
  }, []);

  // Receive the tight model bounds (10% inset) from Live2DCanvas; stored for
  // click hit testing (kept separate from the loose gaze/through rect).
  const handleModelHitBounds = useCallback((b: { x: number; y: number; width: number; height: number }) => {
    modelHitBoundsRef.current = b;
  }, []);

  // Click-through: toggle whether transparent regions forward clicks to the desktop.
  // Safe default: never flip to ignore unless we have all the geometry we need.
  const applyIgnore = useCallback((desired: boolean) => {
    if (ignoreRef.current === desired) return;
    ignoreRef.current = desired;
    getCurrentWindow()
      .setIgnoreCursorEvents(desired)
      .catch((e) => console.warn("[clickthrough] setIgnore failed", e));
  }, []);

  // FSM tick timer: drives microbehaviors independently of LLM
  useEffect(() => {
    const timer = setInterval(() => {
      const fsm = fsmRef.current;
      if (!fsm || awayMode) return;
      if (isThinking || behavior === BehaviorState.Talking) return;
      fsm.tick(moodRef.current, energyRef.current, closenessRef.current, Date.now(), pickNextBehavior);
    }, 2500);
    return () => clearInterval(timer);
 }, [isThinking, behavior, awayMode]);

 // Attention tracking: mouse proximity to pet (Design doc 6.6)
  useEffect(() => {
   const onMouseMove = (e: MouseEvent) => {
     const el = petRef.current;
     if (!el || awayMode) return;
      pointerRef.current = { x: e.clientX, y: e.clientY };
     const rect = el.getBoundingClientRect();
      const petRect: PetRect = {
        centerX: rect.left + rect.width / 2,
        centerY: rect.top + rect.height / 2,
        width: rect.width,
        height: rect.height,
      };
      setAttention(computeAttention(e.clientX, e.clientY, petRect));
    };
    window.addEventListener("mousemove", onMouseMove);
    return () => window.removeEventListener("mousemove", onMouseMove);
  }, [awayMode]);

  // Update emotion + closeness refs for FSM
  useEffect(() => {
    const emoTimer = setInterval(async () => {
      try {
        const emo = await invoke<EmotionData>("get_emotion_state");
        moodRef.current = emo.mood;
        energyRef.current = emo.physical_energy;
      } catch { /* ignore */ }
    }, 10000);
    return () => clearInterval(emoTimer);
  }, []);

  useEffect(() => {
    const snapTimer = setInterval(async () => {
      try {
        const snap = await invoke<Record<string, number>>("get_debug_snapshot");
        closenessRef.current = snap.closeness || 0;
      } catch { /* ignore */ }
    }, 15000);
    return () => clearInterval(snapTimer);
  }, []);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    // StrictMode (dev) mounts effects twice (mount -> cleanup -> remount). Since
    // listen() is async, the first mount's listener can resolve *after* cleanup
    // ran — leaking a duplicate listener that fires on every event. `cancelled`
    // lets a late-resolving listener unlisten itself instead of leaking.
    let cancelled = false;

   listen<string>("bubble-show", (event) => {
      // 访谈进行中：只允许访谈气泡，忽略后端 welcome 等其它气泡以免覆盖当前问题。
      if (onboardingActiveRef.current) return;
      if (welcomeTimerRef.current) { clearTimeout(welcomeTimerRef.current); welcomeTimerRef.current = null; }
      showBubble(event.payload, 8000, "bubble-calm");
   }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen("bubble-hide", () => {
      setBubbleVisible(false);
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen<EmotionData>("emotion-update", (event) => {
      setMoodLabel(event.payload.mood_label);
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen<{ title: string; event_id: string }>("proactive-prompt", (event) => {
      // Backend emitted a due pending event; route through the memory-grounded
      // generator instead of a canned "怎么样啦？" string.
      invoke<string | null>("proactive_bubble")
        .then((reply) => {
          if (reply) showBubble(reply, 15000, "bubble-calm");
        })
        .catch((e) => console.warn("[proactive-prompt] proactive_bubble failed", e));
      void event;
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    // User returned after >5min away (presence LongAway -> Active). Generate a
    // memory-grounded welcome via the LLM (falls back to a rule line backend-side).
    // Suppressed during the onboarding interview and while the pet is resting.
    listen<{ away_secs: number }>("welcome-back", (event) => {
      if (onboardingActiveRef.current) return;
      if (awayMode) return;
      if (welcomeTimerRef.current) { clearTimeout(welcomeTimerRef.current); welcomeTimerRef.current = null; }
      invoke<string | null>("welcome_back_bubble", { awaySecs: event.payload.away_secs })
        .then((reply) => {
          if (reply) showBubble(reply, 10000, bubbleClassForMood(moodLabel));
        })
        .catch((e) => console.warn("[welcome-back] welcome_back_bubble failed", e));
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen<{ status: string; elapsed_secs: number }>("app-status", (event) => {
      if (event.payload.status === "resumed") {
        const hours = Math.round(event.payload.elapsed_secs / 3600);
        const msg = hours > 1 ? `我睡了${hours}个小时……你回来啦~` : "你回来啦~";
        showBubble(msg, 8000, "bubble-calm");
      }
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    const emotionTimer = setInterval(async () => {
      try {
        const emo = await invoke<EmotionData>("get_emotion_state");
        setMoodLabel(emo.mood_label);
      } catch { /* ignore */ }
    }, 5000);

    invoke<EmotionData>("get_emotion_state")
      .then((emo) => setMoodLabel(emo.mood_label))
      .catch(() => {});

    // FIX-J: frontend welcome fallback (2s) if backend bubble-show not received.
    welcomeTimerRef.current = setTimeout(() => {
      showBubble("你好呀！我是你的桌宠～", 8000, "bubble-calm");
      welcomeTimerRef.current = null;
    }, 2000);

    const proactiveTimer = setInterval(async () => {
      if (awayMode) return;
      try {
        // Cold-start interview check (bypasses closeness gate).
        const interview = await invoke<string | null>("check_cold_start");
        if (interview) {
          showBubble(interview, 15000, "bubble-calm");
          return;
        }

        const action = await invoke<ProactiveAction | null>("check_proactive");
        if (action) {
          // Gate passed (closeness/cooldown/focus): generate a memory-grounded
          // proactive bubble via the LLM. Never use canned greetings.
          const reply = await invoke<string | null>("proactive_bubble");
          if (reply) showBubble(reply, 12000, "bubble-calm");
        }
      } catch { /* ignore */ }
    }, 5 * 60 * 1000);

    return () => {
      cancelled = true;
      unlisteners.forEach((un) => un());
      clearInterval(emotionTimer);
      clearInterval(proactiveTimer);
      if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
      if (welcomeTimerRef.current) clearTimeout(welcomeTimerRef.current);
    };
  }, [showBubble, awayMode]);

 useEffect(() => {
   const onKey = (e: KeyboardEvent) => {
     if (e.key === "F12") { e.preventDefault(); setShowDebug((v) => !v); }
   };
   window.addEventListener("keydown", onKey);
   return () => window.removeEventListener("keydown", onKey);
 }, []);

  // FIX-0 guard: detect browser mode (no Tauri backend) and warn once.
  useEffect(() => {
    if (!(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) {
      const t = setTimeout(() => {
        showBubble("请在桌面模式下运行 (npm run tauri dev)", 10000, "bubble-worried");
      }, 1500);
      return () => clearTimeout(t);
    }
  }, [showBubble]);

 // P12: Initialize nest position on first mount
 useEffect(() => {
    // 方案 B: petPos = window top-left in screen coordinates. Place the window at
    // the bottom-right corner above the taskbar on first launch.
    (async () => {
    const win = getCurrentWindow();
    let screenW = 1920;
    let screenH = 1080;
    try {
      // Prefer the Tauri current-monitor size for accuracy across DPI.
       const factor = await win.scaleFactor();
       let monitor = await currentMonitor();
       if (!monitor) {
         const { primaryMonitor } = await import("@tauri-apps/api/window");
         monitor = await primaryMonitor();
       }
       if (monitor) {
         screenW = monitor.size.width / factor;
         screenH = monitor.size.height / factor;
       }
     } catch { /* keep fallback 1920x1080 */ }
      const x = Math.max(20, Math.round(screenW - WINDOW_W - 20));
      const y = Math.max(20, Math.round(screenH - WINDOW_H - 48)); // above taskbar
      const initPos = { x, y };
      try { await win.setPosition(new LogicalPosition(x, y)); } catch (e) { console.warn("[Init] setPosition failed", e); }
      console.log("[Init] window placed at", x, y, "screen:", screenW, screenH);
     spatialRef.current!.setNest(initPos);
     setPetPos(initPos);
    })();
}, []);

  // 首次见面访谈：启动时查后端 needs_onboarding，未完成则开始访谈。
  useEffect(() => {
    (async () => {
      try {
        if (await invoke<boolean>("needs_onboarding")) startOnboarding();
      } catch (e) {
        console.warn("onboarding check", e);
      }
    })();
  }, [startOnboarding]);

  // Soul layer: on startup, trigger reflection if due (>20h) then surface
  // any pending internal thoughts as a bubble. Skipped during onboarding
  // (no conversations to reflect on yet). Runs once.
  useEffect(() => {
    (async () => {
      try {
        // Don't run if onboarding is active (user hasn't had real conversations yet).
        if (await invoke<boolean>("needs_onboarding")) return;

        // Trigger reflection if >20h since last (returns bool).
        const reflected = await invoke<boolean>("trigger_reflection_if_due");
        if (reflected) console.log("[Soul] reflection completed");

        // Surface pending thoughts (marks them surfaced, returns contents).
        const thoughts = await invoke<string[]>("get_pending_thoughts");
        if (thoughts.length > 0) {
          // Delay so the thought bubble doesn't collide with the welcome bubble.
          setTimeout(() => {
            showBubble(thoughts[0], 12000, "bubble-calm");
          }, 6000);
        }
      } catch (e) {
        console.warn("[Soul] reflection/thought check failed", e);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Click-through (ADR Phase 2): transparent regions forward clicks to the desktop.
  // Listens to the backend global-cursor event, compares against the model's screen
  // rect (window origin + canvas-local model bounds * scaleFactor), and toggles
  // setIgnoreCursorEvents. Forces capture (ignore=false) when input/settings/drag are active.
  useEffect(() => {
    const win = getCurrentWindow();
    const refreshOrigin = async () => {
      try {
        const p = await win.outerPosition();
        windowOriginRef.current = { x: p.x, y: p.y };
        const f = await win.scaleFactor();
        scaleFactorRef.current = f;
      } catch { /* leave nulls; safe default keeps window interactive */ }
    };
    refreshOrigin();
    let unlistenMoved: UnlistenFn | undefined;
    win.onMoved(() => { void refreshOrigin(); }).then((u) => { unlistenMoved = u; }).catch(() => {});

    let unlisten: UnlistenFn | undefined;
    listen<{ x: number; y: number }>("global-cursor", (e) => {
      const { x: sx, y: sy } = e.payload; // physical screen px
      const origin = windowOriginRef.current;
      const scale = scaleFactorRef.current;
      const canvas = canvasRectRef.current;
      const mb = modelBoundsRef.current;
      // Force-capture: never ignore when the user needs to interact with the whole window.
      const forceCapture = inputVisible || showSettings || showDebug || isBeingDragged;
      if (forceCapture) {
        applyIgnore(false);
        return;
      }
      // Missing geometry -> stay fully interactive (safe default).
      if (!origin || !scale || !canvas || !mb) {
        applyIgnore(false);
        return;
      }
      const left = origin.x + (canvas.left + mb.x) * scale;
      const top = origin.y + (canvas.top + mb.y) * scale;
      const right = left + mb.width * scale;
      const bottom = top + mb.height * scale;
     const inside = sx >= left && sx <= right && sy >= top && sy <= bottom;
      // global-cursor 是穿透期间的唯一权威指针来源。即使后续 ignore=true，
      // focus ticker 仍读 pointerRef，所以必须持续更新它（client 坐标口径，
      // 与 Live2DCanvas focusTickerFn 里 p.x-rect.left 一致）。
      const clientX = (sx - origin.x) / scale;
      const clientY = (sy - origin.y) / scale;
      pointerRef.current = { x: clientX, y: clientY };
     applyIgnore(!inside);
      if (!inside) setAttention(AttentionState.Ignored);
    }).then((u) => { unlisten = u; }).catch(() => {});

    return () => {
      unlisten?.();
      unlistenMoved?.();
    };
  }, [applyIgnore, inputVisible, showSettings, showDebug, isBeingDragged]);

  // P12: Physics + spatial + circadian loop (Body layer, independent of LLM)
  useEffect(() => {
    let raf = 0;
    let lastTime = performance.now();

   const loop = (now: number) => {
     const dt = Math.min(0.05, (now - lastTime) / 1000); // cap at 50ms
     lastTime = now;

     const spatial = spatialRef.current!;

     if (petPos && !isBeingDragged && !awayMode) {
        // 方案 B: no in-window free-fall. Only spatial "return to nest" moves the
        // OS window smoothly toward the nest position via setPosition.
        const interacting = isThinking || attention === AttentionState.Focused || inputVisible;
        const spatialResult = spatial.tick(petPos, dt, interacting, true);
        if (spatialResult.isWalking) {
          setPetPos(spatialResult.newPos);
          setIsWalking(true);
          // Move the actual window to match (fire-and-forget; batched by the browser).
          getCurrentWindow()
            .setPosition(new LogicalPosition(Math.round(spatialResult.newPos.x), Math.round(spatialResult.newPos.y)))
            .catch(() => {});
        } else {
          setIsWalking(false);
        }
     }

     // Circadian update (every 30s is enough, but cheap to check each frame)
     circadianRef.current = getCircadianState();

     raf = requestAnimationFrame(loop);
   };

   raf = requestAnimationFrame(loop);
   return () => cancelAnimationFrame(raf);
  }, [petPos, isBeingDragged, awayMode, isThinking, attention, inputVisible]);

  // P12: DeepNight proactive nudge (every 10 min check, fires once per period)
  useEffect(() => {
    const timer = setInterval(() => {
      if (awayMode) return;
      const circ = getCircadianState();
      if (circ.period === TimeOfDay.DeepNight && Math.random() < 0.4) {
        const msgs = deepNightMessages();
        showBubble(msgs[Math.floor(Math.random() * msgs.length)], 8000, "bubble-worried");
      } else if (circ.period === TimeOfDay.LateNight && Math.random() < 0.2) {
        showBubble("还不睡呀…", 6000, "bubble-sad");
      }
    }, 10 * 60 * 1000);
    return () => clearInterval(timer);
  }, [showBubble, awayMode]);

// P12: Drag handling
  // 方案 B: drag the pet = drag the OS window. NATIVE startDragging() for zero
  // jitter, but DEFERRED until the pointer actually moves past a threshold — so a
  // pure click (press+release without moving) never enters the OS drag loop and
  // Live2D hit-test clicks (head/body bubbles) still fire normally.
  const DRAG_THRESHOLD = 5;

  const handleDragStart = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return; // left click only
    const startClientX = e.clientX;
    const startClientY = e.clientY;
    let dragStarted = false;
    let onMove: ((ev: MouseEvent) => void) | null = null;
    let onUp: ((ev: MouseEvent) => void) | null = null;

    const cleanup = () => {
      if (onMove) window.removeEventListener("mousemove", onMove);
      if (onUp) window.removeEventListener("mouseup", onUp);
    };

    onMove = (_ev: MouseEvent) => {
      if (dragStarted) return; // OS now owns the drag
      const dx = _ev.clientX - startClientX;
      const dy = _ev.clientY - startClientY;
      if (Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return;
      // Real drag: hand off to OS compositor (in lockstep with pointer, no jitter).
      dragStarted = true;
      setIsBeingDragged(true);
      const win = getCurrentWindow();
      win.startDragging().catch((err) => console.warn("[drag] startDragging failed", err));
      cleanup(); // stop watching; compositor drives movement from here
    };

    onUp = () => {
      cleanup();
      if (!dragStarted) {
        // pure click: nothing to sync, let click/hit-test events proceed.
        return;
      }
      setIsBeingDragged(false);
      const win = getCurrentWindow();
      win.scaleFactor()
        .then((f) => win.outerPosition().then((p) => ({ f, p })))
        .then(({ f, p }) => setPetPos({ x: p.x / f, y: p.y / f }))
        .catch(() => {});
    };
    window.addEventListener("mouseup", onUp);
    window.addEventListener("mousemove", onMove);
  }, []);

  // overrideText lets Escape submit an empty answer during onboarding (see onKeyDown).
  const handleSend = useCallback(async (overrideText?: string) => {
    const text = (overrideText !== undefined ? overrideText : inputText).trim();
    // 访谈分流：存答案→推进→收尾，不走正常对话
    if (onboarding?.active) {
      const { step, answers } = onboarding;
      const key = ONBOARD_QUESTIONS[step].key;
      try { await invoke("save_onboarding_answer", { key, value: text }); }
      catch (e) { console.warn("save_onboarding_answer", e); }
      setInputText("");
      const nextStep = step + 1;
      if (nextStep < ONBOARD_QUESTIONS.length) {
        setOnboarding({ active: true, step: nextStep, answers: { ...answers, [key]: text } });
        showBubble(ONBOARD_QUESTIONS[nextStep].ask, 120000, "bubble-calm");
        requestAnimationFrame(() => {
          document.querySelector<HTMLInputElement>(".input-bubble input")?.focus();
        });
      } else {
        try { await invoke("complete_onboarding"); }
        catch (e) { console.warn("complete_onboarding", e); }
        onboardingActiveRef.current = false;
        setOnboarding(null);
        showBubble("认识你真高兴！以后就这么陪着你啦~", 10000, "bubble-happy");
      }
      return;
    }
    if (!text) { setInputVisible(false); return; }
    setInputText("");
    setInputVisible(false); // 轮次制：发送后立即收起输入框，让桌宠回复气泡独占显示
   setIsThinking(true);
   fsmRef.current?.forceState(BehaviorState.Thinking);
    thinkStartRef.current = Date.now();

   try {
     const res = await invoke<{ reply: string; transient_expression: string | null }>("send_message", { text });
     const reply = res.reply;
      // FIX-G: guarantee the thinking dots stay visible >= 500ms even on fast replies.
      const elapsed = Date.now() - thinkStartRef.current;
      if (elapsed < 500) await new Promise((r) => setTimeout(r, 500 - elapsed));
     setIsThinking(false);
      fsmRef.current?.forceState(BehaviorState.Talking);
     if (reply) {
       showBubble(reply, 10000, bubbleClassForMood(moodLabel));
     } else {
        showBubble("（……）", 3000, "bubble-calm");
     }
     setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 2000);
     if (res.transient_expression) {
       if (transientTimerRef.current) clearTimeout(transientTimerRef.current);
       setTransientExpression(res.transient_expression);
       transientTimerRef.current = window.setTimeout(() => setTransientExpression(null), 8000);
     }
      // Refresh emotion immediately so the expression changes right after the
      // reply, instead of waiting up to 5s for the next poll.
      invoke<EmotionData>("get_emotion_state")
        .then((emo) => setMoodLabel(emo.mood_label))
        .catch(() => {});
   } catch (e) {
      // FIX-G: keep the dots up for >= 500ms even when it fails fast.
      const elapsed = Date.now() - thinkStartRef.current;
      if (elapsed < 500) await new Promise((r) => setTimeout(r, 500 - elapsed));
     setIsThinking(false);
     fsmRef.current?.forceState(BehaviorState.Idle);
      // FIX-F: surface a readable error instead of a bare "...", and log full detail.
      console.error("[Conversation] send_message failed:", e);
     const errMsg = String(e);
     if (errMsg.includes("not configured") || errMsg.includes("NotConfigured")) {
        showBubble("（还没有配置好连接……）", 5000, "bubble-worried");
     } else if (errMsg.includes("Timeout") || errMsg.includes("timeout")) {
        showBubble("我……刚刚有点走神……", 5000, "bubble-worried");
     } else if (errMsg.includes("Network") || errMsg.includes("network")) {
        showBubble("信号不太好呢……", 5000, "bubble-worried");
     } else if (errMsg.includes("RateLimit") || errMsg.includes("429")) {
        showBubble("说了好多话，让我喘口气吧~", 5000, "bubble-calm");
     } else {
        showBubble("（连接出了点问题…）", 5000, "bubble-worried");
    }
  }
}, [inputText, showBubble, moodLabel, onboarding]);

 // Head pet: closeness + mood up, 3s cooldown
 const handleHeadClick = useCallback(() => {
   const now = Date.now();
   if (now - lastPetTimeRef.current < 3000) return;
   lastPetTimeRef.current = now;
    // 单/双击消歧：与 body 一致，延迟 280ms，双击取消
    if (pendingPokeRef.current) clearTimeout(pendingPokeRef.current);
    pendingPokeRef.current = setTimeout(() => {
      pendingPokeRef.current = null;
      invoke("pet_head").catch(() => {});
      fsmRef.current?.transition(BehaviorState.Embarrassed);
      const reactions = ["嘿嘿…", "谢谢你～", "抹抹~"];
      showBubble(reactions[Math.floor(Math.random() * reactions.length)], 3000, "bubble-happy", "bubble-pet");
      setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 1500);
    }, 280);
 }, [showBubble]);

// Body poke: mood down after 3 pokes
const handleBodyClick = useCallback(() => {
  if (inputVisible) return; // 输入框打开时不弹身体气泡
  // 单/双击消歧：戳反应延迟 280ms，双击会在 dblclick 里取消它
  if (pendingPokeRef.current) clearTimeout(pendingPokeRef.current);
  pendingPokeRef.current = setTimeout(() => {
    pendingPokeRef.current = null;
    pokeCountRef.current++;
    if (pokeResetTimerRef.current) clearTimeout(pokeResetTimerRef.current);
    pokeResetTimerRef.current = setTimeout(() => { pokeCountRef.current = 0; }, 5000);
    fsmRef.current?.transition(BehaviorState.Embarrassed);
    setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 1500);
    const n = pokeCountRef.current;
    if (n === 1) {
      showBubble("？", 2500, "bubble-worried");
    } else if (n === 2) {
      showBubble("嗯…", 2500, "bubble-worried");
    } else {
      showBubble("别戳了啦！", 3000, "bubble-worried");
    }
    invoke<boolean>("poke", { count: pokeCountRef.current }).catch(() => {});
  }, 280);
}, [showBubble, inputVisible]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  const handleExportMemory = useCallback(async () => {
    showBubble("记忆导出中…", 3000, "bubble-calm");
    try {
      const snap = await invoke("get_debug_snapshot");
      const json = JSON.stringify(snap, null, 2);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "pet-memory-export.json";
      a.click();
      URL.revokeObjectURL(url);
      showBubble("记忆已保存～", 3000, "bubble-happy");
    } catch (e) {
      console.error("[Export]", e);
      showBubble("导出失败了…", 3000, "bubble-worried");
    }
  }, [showBubble]);

  const handleAwayMode = useCallback(() => {
    setAwayMode(true);
    showBubble("我先休息一下哦~", 4000, "bubble-calm");
  }, [showBubble]);

  const handleQuit = useCallback(() => {
    showBubble("再见…", 3000, "bubble-sad");
    // FIX: previously only showed the bubble and never closed. We now call a
    // Rust-side quit_app (app.exit(0)) which terminates the process
    // deterministically — window.destroy() alone did not reliably exit under
    // Tauri 2. 400ms lets the goodbye render before the process goes away.
    setTimeout(() => { void invoke("quit_app"); }, 400);
  }, [showBubble]);

  return (
    <div className="pet-container" onContextMenu={handleContextMenu}>
      {isThinking && (
        <div className="thinking-dots">
          <span />
          <span />
          <span />
        </div>
      )}

      <div className="input-bubble">
          <input
            type="text"
            value={inputText}
            autoFocus
            placeholder={"想和我说什么？"}
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={(e) => {
             if (e.key === "Enter") handleSend();
              if (e.key === "Escape") {
                // 访谈中 Escape 当作空答案，推进到下一题
                if (onboarding?.active) handleSend("");
                else setInputVisible(false);
              }
            }}
            // Input stays open during conversation; closed only by Esc or empty-message Enter.
            // 轮次制：失焦即收起输入框；访谈期间保持打开
            onBlur={() => {}}
          />
      </div>

      <div className={`chat-bubble ${bubbleVisible ? "" : "hidden"} ${bubbleStyle} ${bubblePos}`}>
        {bubbleText}
      </div>

     <div
       ref={petRef}
      className={`pet-char-wrapper ${isWalking ? "walking" : ""} ${isBeingDragged ? "dragging" : ""}`}
      onDoubleClick={() => {
        if (pendingPokeRef.current) { clearTimeout(pendingPokeRef.current); pendingPokeRef.current = null; }
        setBubbleVisible(false);
        if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
        setInputVisible(true);
      }}
      onMouseDown={handleDragStart}
   >
    <Live2DCanvas
      moodLabel={moodLabel}
       behavior={behavior}
       attention={attention}
       pointerRef={pointerRef}
     isThinking={isThinking}
     onHeadClick={handleHeadClick}
     onBodyClick={handleBodyClick}
     onModelBounds={handleModelBounds}
     onModelHitBounds={handleModelHitBounds}
     transientExpression={transientExpression}
   />
   </div>

      <button
        className="settings-btn"
        onClick={() => setShowSettings(true)}
        title="Settings"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>

      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onExportMemory={handleExportMemory}
          onAwayMode={handleAwayMode}
         onQuit={handleQuit}
          onDevTools={() => invoke("open_devtools")}
       />
      )}

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showDebug && <DebugPanel />}
    </div>
  );
}
