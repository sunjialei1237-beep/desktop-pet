import { useState, useEffect, useCallback, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
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
import { createGravity, stepGravity, type GravityState } from "./animation/gravity";
import { getCircadianState, deepNightMessages, TimeOfDay } from "./animation/circadian";
import { DEFAULT_EMOTION, type EmotionVector } from "./animation/emotionDriver";
import { typewriterPacing, inferPacingMood } from "./animation/bubblePacing";
import { shouldAutoSleep } from "./animation/sleepLogic";
import { sound, INTIMATE_THRESHOLD } from "./audio/soundManager";

interface EmotionData {
  mood: number;
  mood_label: string;
  physical_energy: number;
  social_battery: number;
  stress: number;
  loneliness: number;
  rest_need: number;
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

// Project the backend emotion snapshot to the continuous vector the Live2D
// layer interpolates. `rest_need` is now exposed by the backend (and evolved in
// the homeostasis loop), so a tired pet's droopy eyes actually show.
function toEmotionVector(e: EmotionData): EmotionVector {
  return {
    mood: e.mood,
    physical_energy: e.physical_energy,
    social_battery: e.social_battery,
    stress: e.stress,
    loneliness: e.loneliness,
    rest_need: e.rest_need,
  };
}

export default function App() {
// 方案 B: window dimensions match tauri.conf.json. petPos = window top-left.
const WINDOW_W = 400;
const WINDOW_H = 760;
// Sleeping: in DeepNight (2-6) the pet drifts off after this long with no
// interaction. Any interaction refreshes it -> natural awake cooldown (#10).
const SLEEP_AFTER_IDLE_MS = 10 * 60 * 1000;

  const [bubbleText, setBubbleText] = useState("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  const [bubbleStyle, setBubbleStyle] = useState("");
  const [bubblePos, setBubblePos] = useState("");
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  const [isThinking, setIsThinking] = useState(false);
  const [transientExpression, setTransientExpression] = useState<string | null>(null);
  const [moodLabel, setMoodLabel] = useState("平静");
  const [emotionVector, setEmotionVector] = useState<EmotionVector>(DEFAULT_EMOTION);
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
  const lastInteractionRef = useRef(Date.now()); // drives DeepNight auto-sleep
  const closenessRef = useRef(0);
  const moodRef = useRef(0.5);
  const energyRef = useRef(0.7);
  // B2 (P12.1): free-fall + taskbar bounce. gravityRef drives the rAF loop
  // after a release mid-air; floorYRef/winSizeRef are refreshed on every
  // release so monitor changes are picked up.
  const gravityRef = useRef<GravityState>(createGravity());
  const floorYRef = useRef(0);
  const winSizeRef = useRef({ w: 0, h: 0 });
  // B2 (P12.1): free-fall stops after a THIRD of the drop distance (user
  // preference 08-01) — she floats to a hover instead of hitting the floor.
  // Set when a fall starts, cleared when she stops (hover or land).
  const fallLimitBottomRef = useRef(0);
  // Native OS dragging (startDragging) swallows all webview mouse events, so
  // drag-end is detected via window "moved" events + a quiet-period check in
  // the rAF loop: after the window stops moving for ~300ms, if she was left
  // mid-air, free-fall starts.
  const wasDraggedRef = useRef(false);
  const lastMovedRef = useRef(0);
  // The physics loop reads/writes position via refs (not state) so moved events
  // and setPosition calls never re-create the loop mid-motion — the previous
  // state-driven deps re-created it every frame, resetting `lastTime` and
  // making dt jitter (visible as stutter while falling/bouncing).
  const petPosRef = useRef<PetPosition | null>(null);
  const isBeingDraggedRef = useRef(false);
  const lastOriginRefreshRef = useRef(0);
 const [isBeingDragged, setIsBeingDragged] = useState(false);
 const [soundMuted, setSoundMuted] = useState(false);
 const circadianRef = useRef(getCircadianState());
 const pointerRef = useRef({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
 const thinkStartRef = useRef(0);
 // Streaming-typewriter state: chunks arrive faster than the eye can follow
 // and React batches same-tick setState, so buffer in refs and reveal on an
 // interval for a smooth typewriter effect (#10).
 const streamBufRef = useRef("");
 const shownLenRef = useRef(0);
 const streamEndedRef = useRef(false);
 const typewriterRef = useRef<number | null>(null);
 // Ref mirrors of bubble/thinking visibility, so the long-lived emotionTimer
 // setInterval reads fresh values instead of a stale mount-time closure
 // (used by the idle-sigh guard below).
 const bubbleVisibleRef = useRef(false);
 const isThinkingRef = useRef(false);
 useEffect(() => { bubbleVisibleRef.current = bubbleVisible; }, [bubbleVisible]);
 useEffect(() => { isThinkingRef.current = isThinking; }, [isThinking]);
 // Foley: preload all buffers + startup greeting on mount (greeting defers to
 // the first pointerdown if AudioContext is suspended — autoplay policy). Mute
 // mirrors the singleton so the menu toggle and play() agree (#6/#11).
 useEffect(() => {
   sound.preload();
   sound.greet();
 }, []);
 useEffect(() => { sound.setMuted(soundMuted); }, [soundMuted]);
  // Click-through state (ADR Phase 2).
  const ignoreRef = useRef(false);
  const windowOriginRef = useRef<{ x: number; y: number } | null>(null); // physical px
  const scaleFactorRef = useRef<number | null>(null);
  const modelBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const modelHitBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const canvasRectRef = useRef<{ left: number; top: number } | null>(null);

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
      // DeepNight (2-6) auto-sleep: left alone long enough + nothing happening ->
      // drift off. markInteraction() refreshes lastInteractionRef on any poke/pet/
      // drag/chat, so this won't re-fire until SLEEP_AFTER_IDLE_MS elapses again.
      // Condition extracted to shouldAutoSleep() so it is unit-testable.
      if (
        shouldAutoSleep({
          period: circadianRef.current.period,
          state: fsm.state,
          isThinking,
          isTalking: behavior === BehaviorState.Talking,
          idleMs: Date.now() - lastInteractionRef.current,
          thresholdMs: SLEEP_AFTER_IDLE_MS,
        })
      ) {
        fsm.forceState(BehaviorState.Sleeping);
        sound.sleep(); // drifting-off cue (plays once — guard above gates entry)
        return;
      }
      if (isThinking || behavior === BehaviorState.Talking) return;
      fsm.tick(moodRef.current, energyRef.current, closenessRef.current, circadianRef.current.sleepiness, Date.now(), pickNextBehavior);
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
      setEmotionVector(toEmotionVector(event.payload));
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

    // Loneliness-driven nudge: homeostasis let loneliness climb while the user
    // was idle at the desk, and the backend (gated by closeness + presence +
    // 30-min cooldown) decided she should reach out. Voice it via the
    // memory-grounded LLM path (falls back to a rule line backend-side).
    // Suppressed during onboarding / away, like welcome-back.
    listen<{ loneliness: number }>("lonely-nudge", () => {
      if (onboardingActiveRef.current) return;
      if (awayMode) return;
      // She's asleep — don't wake her to say "想你了". Mirrors the "go to bed"
      // nudge guard: a sleepy 璃 stays sleepy (Architecture #12 silence).
      if (fsmRef.current?.state === BehaviorState.Sleeping) return;
      invoke<string | null>("lonely_bubble")
        .then((reply) => {
          if (reply) showBubble(reply, 10000, bubbleClassForMood(moodLabel));
        })
        .catch((e) => console.warn("[lonely-nudge] lonely_bubble failed", e));
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
        setEmotionVector(toEmotionVector(emo));
        // Idle sigh (architecture #12 silence-is-expression, #10 liveliness):
        // when she's worn down and not busy, occasionally let out a wordless
        // "呼…" glyph bubble. Guards keep it from interrupting an active
        // conversation / interview / away mode. Uses the just-fetched label
        // (fresh) and ref mirrors (fresh), not the stale closure state.
        if (
          (emo.mood_label === "疲惫" || emo.mood_label === "难过") &&
          !bubbleVisibleRef.current &&
          !isThinkingRef.current &&
          !onboardingActiveRef.current &&
          !awayMode &&
          Math.random() < 0.08
        ) {
          showBubble("呼…", 2500, "bubble-glyph");
        }
      } catch { /* ignore */ }
    }, 5000);

    invoke<EmotionData>("get_emotion_state")
      .then((emo) => {
        setMoodLabel(emo.mood_label);
        setEmotionVector(toEmotionVector(emo));
      })
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
     // F12 is the default, but some laptops hijack it (e.g. sleep key), so also
     // accept Ctrl+Shift+D as a reliable alternate to toggle the Debug Panel.
     const k = e.key.toLowerCase();
     if (e.key === "F12" || (e.ctrlKey && e.shiftKey && k === "d")) {
       e.preventDefault();
       setShowDebug((v) => !v);
     }
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
    let factor = 1;
    try {
      // Prefer the Tauri current-monitor size for accuracy across DPI.
       factor = await win.scaleFactor();
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
     petPosRef.current = initPos;
     // B2 (P12.1): cache window size + work-area bottom (taskbar top) once —
     // the fall/land physics collide against this floor. Refreshed again on
     // each drag release via currentMonitor.
     try {
       const size = await win.outerSize();
       winSizeRef.current = { w: size.width / factor, h: size.height / factor };
       const mon = await currentMonitor();
       if (mon) {
         floorYRef.current = mon.workArea.position.y / factor + mon.workArea.size.height / factor;
       }
     } catch { /* keep defaults; drag release re-measures */ }
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
    // B2 (P12.1): native startDragging swallows webview mouse events entirely
    // (verified: no mouseup ever reaches the page), so drag-end is detected
    // via movement quiescence: onMoved keeps lastMovedRef fresh; the rAF loop
    // starts free-fall after ~300ms of stillness. onMoved also keeps petPos in
    // sync with the real window position (previously stale after every drag).
    win.onMoved(({ payload: pos }) => {
      const f = scaleFactorRef.current || 1;
      const logical = { x: pos.x / f, y: pos.y / f };
      // While free-falling the rAF loop owns the position (it already writes
      // the rounded value it setPosition'd), so skip the echo to avoid
      // fighting the loop; when grounded, keep petPosRef in sync.
      if (gravityRef.current.grounded) {
        petPosRef.current = logical;
      }
      lastMovedRef.current = performance.now();
      if (isBeingDraggedRef.current) {
        wasDraggedRef.current = true;
        isBeingDraggedRef.current = false;
        setIsBeingDragged(false);
      }
      // Throttle the click-through origin refresh: it does an async
      // outerPosition IPC per move, which floods during fast motion
      // (drag/fall/walk) and causes visible stutter.
      const now = performance.now();
      if (now - lastOriginRefreshRef.current > 100) {
        lastOriginRefreshRef.current = now;
        void refreshOrigin();
      }
    }).then((u) => { unlistenMoved = u; }).catch(() => {});

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

  // P12: Physics + circadian loop (Body layer, independent of LLM)
  useEffect(() => {
    let raf = 0;
    let lastTime = performance.now();

   const loop = (now: number) => {
     const dt = Math.min(0.05, (now - lastTime) / 1000); // cap at 50ms
     lastTime = now;

      const gravity = gravityRef.current;
      const pos = petPosRef.current;

      if (pos && !isBeingDraggedRef.current && !awayMode) {
        // B2 (P12.1): free-fall toward a hover point (1/3 of the way to the
        // taskbar). Runs until grounded.
        if (!gravity.grounded) {
          const win = getCurrentWindow();
          const bottom = pos.y + winSizeRef.current.h;
          const newBottom = stepGravity(gravity, dt, bottom);
          // 1/3-arc rule: she falls only a third of the way to the floor, then
          // floats to a stop (hover). Reaching the hover point plays the
          // settling sound.
          let finalBottom = newBottom;
          const limit = fallLimitBottomRef.current;
          if (limit > 0 && newBottom >= limit) {
            finalBottom = limit;
            gravity.grounded = true;
            gravity.vy = 0;
            fallLimitBottomRef.current = 0;
            sound.play("land"); // reached the hover point — soft settling sound
          }
          const newY = Math.round(finalBottom - winSizeRef.current.h);
          petPosRef.current = { x: pos.x, y: newY };
          win.setPosition(new LogicalPosition(pos.x, newY)).catch(() => {});
        } else {
          // B2 (P12.1): drag-end detection. After a native drag the window
          // goes still once the user releases; if she was left above the
          // work-area bottom, start free-fall.
          if (wasDraggedRef.current && now - lastMovedRef.current > 300) {
            wasDraggedRef.current = false;
            if (pos.y + winSizeRef.current.h < floorYRef.current - 2) {
              gravity.grounded = false;
              gravity.vy = 0;
              // Only fall a third of the way to the floor (user preference).
              fallLimitBottomRef.current =
                pos.y + winSizeRef.current.h + (floorYRef.current - (pos.y + winSizeRef.current.h)) / 3;
            } else {
              sound.play("land"); // dropped right on the floor: thud now
            }
          }
          // (2026-08-08) Walk-back-to-nest removed — 桌宠驻留，拖到哪停哪，
          // 不再自主走回角落窝。Body 层只剩自由落体 + 落地弹跳 + 昼夜节律。
        }
      }

     // Circadian update (every 30s is enough, but cheap to check each frame)
     circadianRef.current = getCircadianState();

     raf = requestAnimationFrame(loop);
   };

   raf = requestAnimationFrame(loop);
   return () => cancelAnimationFrame(raf);
  }, [awayMode]);

  // P12: DeepNight/LateNight proactive nudge. Extracted into a callback so the
  // dev verify hook (window.__pet.probeNudge) can fire one on demand instead of
  // waiting the full 10-min interval.
  const runNudge = useCallback(() => {
    if (awayMode) return;
    // She's asleep — don't sleep-talk the "go to bed" nudge (#10).
    if (fsmRef.current?.state === BehaviorState.Sleeping) return;
    const circ = getCircadianState();
    if (circ.period === TimeOfDay.DeepNight && Math.random() < 0.4) {
      const msgs = deepNightMessages();
      showBubble(msgs[Math.floor(Math.random() * msgs.length)], 8000, "bubble-worried");
    } else if (circ.period === TimeOfDay.LateNight && Math.random() < 0.2) {
      showBubble("还不睡呀…", 6000, "bubble-sad");
    }
  }, [showBubble, awayMode]);

  // Nudge check every 10 min (fires once per period, probabilistically).
  useEffect(() => {
    const timer = setInterval(runNudge, 10 * 60 * 1000);
    return () => clearInterval(timer);
  }, [runNudge]);

// P12: Drag handling
  // 方案 B: drag the pet = drag the OS window. NATIVE startDragging() for zero
  // jitter, but DEFERRED until the pointer actually moves past a threshold — so a
  // pure click (press+release without moving) never enters the OS drag loop and
  // Live2D hit-test clicks (head/body bubbles) still fire normally.
  const DRAG_THRESHOLD = 5;

  // Any user interaction refreshes the DeepNight sleep-idle timer and wakes the
  // pet if asleep. Stable (deps []): only touches refs, so callers may omit it
  // from their own deps without stale-closure risk. forceState bypasses the
  // transition priority lock so Sleeping can exit.
  const markInteraction = useCallback(() => {
    lastInteractionRef.current = Date.now();
    if (fsmRef.current?.state === BehaviorState.Sleeping) {
      fsmRef.current.forceState(BehaviorState.Idle);
    }
  }, []);

  // DEV-ONLY verify hook (北极星 #7: invisible in release — Vite replaces
  // import.meta.env.DEV with false and dead-code-eliminates this effect).
  // Lets us验收 circadian / Sleeping / B3 WITHOUT touching the OS clock or
  // waiting 10 min. Open browser DevTools (right-click pet → DevTools, dev
  // only — calls open_devtools; NOTE F12 is the in-app Debug Panel, different)
  // → Console → window.__pet. See docs/verify-checklist.md.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const origGetHours = Date.prototype.getHours;
    const api = {
      // Pretend it's hour h. circadian.ts is the only getHours caller; Date.now()
      // is untouched, so idle/sleep timers still use the real clock.
      setHour: (h: number): void => { Date.prototype.getHours = (): number => h; },
      // Restore the real clock.
      resetHour: (): void => { Date.prototype.getHours = origGetHours; },
      // Back-date last interaction so the DeepNight auto-sleep guard fires on
      // the next 2.5s tick — skips the 10-min idle wait, real code path.
      forceIdle: (mins: number): void => {
        lastInteractionRef.current = Date.now() - mins * 60_000;
      },
      // Enter/exit Sleeping directly (sleeping pose + sleep sound).
      sleep: (): void => {
        fsmRef.current?.forceState(BehaviorState.Sleeping);
        sound.sleep();
      },
      wake: (): void => { markInteraction(); },
      // Fire one nudge check now instead of waiting 10 min. No-op while asleep
      // (verifies B3① suppression); 0.4/0.2 random, so call a few times.
      probeNudge: (): void => { runNudge(); },
      state: (): Record<string, unknown> => ({
        behavior: fsmRef.current?.state,
        period: circadianRef.current.period,
        sleepiness: circadianRef.current.sleepiness,
        idleSecs: Math.round((Date.now() - lastInteractionRef.current) / 1000),
      }),
    };
    const w = window as unknown as { __pet?: typeof api };
    w.__pet = api;
    console.log("[dev] window.__pet ready: setHour/forceIdle/sleep/wake/probeNudge/state");
    return () => {
      Date.prototype.getHours = origGetHours; // never leak a fake hour
      delete w.__pet;
    };
  }, [runNudge, markInteraction]);

  const handleDragStart = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return; // left click only
    markInteraction();
    const startClientX = e.clientX;
    const startClientY = e.clientY;
    let dragStarted = false;
    let onMove: ((ev: MouseEvent) => void) | null = null;

    const cleanup = () => {
      if (onMove) window.removeEventListener("mousemove", onMove);
    };

    onMove = (_ev: MouseEvent) => {
      if (dragStarted) return; // OS now owns the drag
      const dx = _ev.clientX - startClientX;
      const dy = _ev.clientY - startClientY;
      if (Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return;
      // Real drag: hand off to OS compositor (in lockstep with pointer, no jitter).
      dragStarted = true;
      wasDraggedRef.current = true;
      isBeingDraggedRef.current = true;
      setIsBeingDragged(true);
      sound.play("drag");
      const win = getCurrentWindow();
      win.startDragging().catch((err) => console.warn("[drag] startDragging failed", err));
      cleanup(); // stop watching; compositor drives movement from here.
      // NOTE: no mouseup listener — native dragging swallows webview mouse
      // events entirely, so drag-end is detected via onMoved + quiet period
      // (see the gravity effect below).
    };

    window.addEventListener("mousemove", onMove);
  }, []);

  // overrideText lets Escape submit an empty answer during onboarding (see onKeyDown).
  const handleSend = useCallback(async (overrideText?: string) => {
    markInteraction();
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
    sound.play("send");
   setIsThinking(true);
   fsmRef.current?.forceState(BehaviorState.Thinking);
    thinkStartRef.current = Date.now();

   try {
     // Stream tokens live: the first content chunk means she starts speaking
     // (drop dots, switch to Talking, open the bubble); later chunks accumulate
     // into the bubble text. DeepSeek emits reasoning before content, so dots
     // stay up while she "thinks" (architecture #10, 踩坑#3).
     // Stream tokens via a Tauri event. Chunks arrive faster than the eye can
     // follow (DeepSeek emits ~80 tokens/s) and React batches same-tick
     // setState into one render — so direct accumulation visibly "jumps". We
     // buffer into a ref and reveal on a 30ms interval => smooth typewriter
     // (architecture #10).
     streamBufRef.current = "";
     shownLenRef.current = 0;
     streamEndedRef.current = false;
     if (typewriterRef.current) { window.clearInterval(typewriterRef.current); typewriterRef.current = null; }

     let firstChunk = true;
     const onChunk = new Channel<string>();
     onChunk.onmessage = (delta: string) => {
       streamBufRef.current += delta; // buffer only — no setState here (defeats batching)
       if (firstChunk) {
         firstChunk = false;
         // Typewriter cadence follows THIS turn's emotion (#10): happy input
         // flows fast, sad/worried drags with pauses. The backend moodLabel is
         // a slow variable (only re-derived after the reply), so we infer the
         // pacing mood from the user's own words for immediacy; moodLabel is
         // just the fallback when no emotion keyword is present.
         const pacing = typewriterPacing(inferPacingMood(text, moodLabel));
         setIsThinking(false);
         fsmRef.current?.forceState(BehaviorState.Talking);
         setBubbleStyle(bubbleClassForMood(moodLabel));
         setBubblePos("");
         setBubbleText("");
         setBubbleVisible(true);
         if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current); // no auto-hide mid-stream
         typewriterRef.current = window.setInterval(() => {
           // Hesitate: while still streaming, skip a tick now and then = a
           // pause (hesitation / breath). Never hesitate once ended — the
           // final reveal must complete.
           if (!streamEndedRef.current && pacing.hesitate > 0 && Math.random() < pacing.hesitate) return;
           const buf = streamBufRef.current;
           if (shownLenRef.current < buf.length) {
             const step = Math.max(1, Math.ceil((buf.length - shownLenRef.current) / pacing.catchDiv));
             shownLenRef.current += step;
             setBubbleText(buf.slice(0, shownLenRef.current));
           } else if (streamEndedRef.current) {
             if (typewriterRef.current) { window.clearInterval(typewriterRef.current); typewriterRef.current = null; }
             bubbleTimerRef.current = setTimeout(() => setBubbleVisible(false), 10000);
             setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 2000);
           }
         }, pacing.intervalMs);
       }
     };
     const res = await invoke<{ reply: string; transient_expression: string | null }>(
       "send_message",
       { text, onChunk },
     );
     // Stream done: correct the buffer to the authoritative full reply (in
     // case the last chunks raced the unlisten), then let the typewriter
     // finish revealing it.
     streamBufRef.current = res.reply;
     streamEndedRef.current = true;

     if (firstChunk) {
       // No content tokens arrived (silence / empty reply): the typewriter
       // never started — fall back to the original thinking→reply path.
       const elapsed = Date.now() - thinkStartRef.current;
       if (elapsed < 500) await new Promise((r) => setTimeout(r, 500 - elapsed));
       setIsThinking(false);
       fsmRef.current?.forceState(BehaviorState.Talking);
       if (res.reply) showBubble(res.reply, 10000, bubbleClassForMood(moodLabel));
       else showBubble("…", 2500, "bubble-glyph");
       setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 2000);
     }
     if (res.transient_expression) {
       if (transientTimerRef.current) clearTimeout(transientTimerRef.current);
       setTransientExpression(res.transient_expression);
       transientTimerRef.current = window.setTimeout(() => setTransientExpression(null), 8000);
     }
      // Refresh emotion immediately so the expression changes right after the
      // reply, instead of waiting up to 5s for the next poll.
      invoke<EmotionData>("get_emotion_state")
        .then((emo) => {
          setMoodLabel(emo.mood_label);
          setEmotionVector(toEmotionVector(emo));
        })
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
   markInteraction();
   const now = Date.now();
   if (now - lastPetTimeRef.current < 3000) return;
   lastPetTimeRef.current = now;
    // 单/双击消歧：与 body 一致，延迟 280ms，双击取消
    if (pendingPokeRef.current) clearTimeout(pendingPokeRef.current);
    pendingPokeRef.current = setTimeout(() => {
      pendingPokeRef.current = null;
      sound.play(closenessRef.current >= INTIMATE_THRESHOLD ? "pet-intimate" : "pet-stranger");
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
  markInteraction();
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
    sound.play(n >= 3 ? "poke3" : n === 2 ? "poke2" : "poke1");
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
    sound.play("menu");
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
    // Hide the window to the system tray shortly after the bubble shows.
    // Clicking the tray icon restores it (clears awayMode via the
    // restore-from-tray listener below).
    setTimeout(() => { void invoke("hide_to_tray"); }, 600);
  }, [showBubble]);

  // Restore from tray: clicking the tray icon re-shows the window (Rust) and
  // emits this event. Clear awayMode and greet the user back.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    (async () => {
      unlisten = await listen("restore-from-tray", () => {
        setAwayMode(false);
        showBubble("回来啦~", 3000, "bubble-happy");
      });
      if (cancelled) unlisten();
    })();
    return () => { cancelled = true; unlisten?.(); };
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
      className={`pet-char-wrapper ${isBeingDragged ? "dragging" : ""}`}
      data-behavior={behavior}
      onDoubleClick={() => {
        markInteraction();
        sound.play("dblclick");
        if (pendingPokeRef.current) { clearTimeout(pendingPokeRef.current); pendingPokeRef.current = null; }
        setBubbleVisible(false);
        if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
        setInputVisible(true);
      }}
      onMouseDown={handleDragStart}
   >
    <Live2DCanvas
      emotionVector={emotionVector}
       behavior={behavior}
       attention={attention}
       pointerRef={pointerRef}
     isThinking={isThinking}
     onHeadClick={handleHeadClick}
     onBodyClick={handleBodyClick}
     onModelBounds={handleModelBounds}
     onModelHitBounds={handleModelHitBounds}
     transientExpression={transientExpression}
     speedModifier={circadianRef.current.speedModifier}
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
          soundMuted={soundMuted}
          onToggleSound={() => setSoundMuted(sound.toggleMuted())}
         onQuit={handleQuit}
          onDevTools={() => invoke("open_devtools")}
       />
      )}

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showDebug && (
        <DebugPanel
          anim={{ state: behavior, history: fsmRef.current?.getHistory() ?? [] }}
          onClose={() => setShowDebug(false)}
          onQuit={handleQuit}
        />
      )}
    </div>
  );
}
