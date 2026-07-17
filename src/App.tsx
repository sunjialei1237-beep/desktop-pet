import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Live2DCanvas } from "./Live2DCanvas";
import { SettingsPanel } from "./SettingsPanel";
import { DebugPanel } from "./DebugPanel";
import { ContextMenu } from "./ContextMenu";
import { AnimationFSM, BehaviorState } from "./animation/fsm";
import { pickNextBehavior } from "./animation/microBehavior";
import { AttentionState, computeAttention, type PetRect } from "./animation/attention";
import { Physics, type PetPosition } from "./animation/physics";
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
  const [bubbleText, setBubbleText] = useState("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  const [bubbleStyle, setBubbleStyle] = useState("");
  const [bubblePos, setBubblePos] = useState("");
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  const [isThinking, setIsThinking] = useState(false);
  const [moodLabel, setMoodLabel] = useState("平静");
  const [showSettings, setShowSettings] = useState(false);
  const [showDebug, setShowDebug] = useState(false);
 const [attention, setAttention] = useState(AttentionState.Ignored);
 const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const [awayMode, setAwayMode] = useState(false);
  const bubbleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const welcomeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [behavior, setBehavior] = useState<BehaviorState>(BehaviorState.Idle);
  const fsmRef = useRef<AnimationFSM | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const pokeCountRef = useRef(0);
  const pokeResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPetTimeRef = useRef(0);
  const closenessRef = useRef(0);
  const moodRef = useRef(0.5);
  const energyRef = useRef(0.7);
  const physicsRef = useRef<Physics | null>(null);
  const spatialRef = useRef<SpatialMemory | null>(null);
  const [petPos, setPetPos] = useState<PetPosition | null>(null);
 const [isWalking, setIsWalking] = useState(false);
 const [isBeingDragged, setIsBeingDragged] = useState(false);
 const circadianRef = useRef(getCircadianState());
  const pointerRef = useRef({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
  const dragStartRef = useRef<{ x: number; y: number; offsetX: number; offsetY: number } | null>(null);
  const dragPendingRef = useRef(false);
  const thinkStartRef = useRef(0);

  if (!physicsRef.current) {
    physicsRef.current = new Physics();
  }
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

  // FSM tick timer: drives microbehaviors independently of LLM (Principle 5)
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

    listen<string>("bubble-show", (event) => {
      if (welcomeTimerRef.current) { clearTimeout(welcomeTimerRef.current); welcomeTimerRef.current = null; }
      showBubble(event.payload, 8000, "bubble-calm");
    }).then((un) => unlisteners.push(un));

    listen("bubble-hide", () => {
      setBubbleVisible(false);
    }).then((un) => unlisteners.push(un));

    listen<EmotionData>("emotion-update", (event) => {
      setMoodLabel(event.payload.mood_label);
    }).then((un) => unlisteners.push(un));

    listen<{ title: string; event_id: string }>("proactive-prompt", (event) => {
      showBubble(event.payload.title + "怎么样啦？", 15000, "bubble-calm");
    }).then((un) => unlisteners.push(un));

    listen<{ status: string; elapsed_secs: number }>("app-status", (event) => {
      if (event.payload.status === "resumed") {
        const hours = Math.round(event.payload.elapsed_secs / 3600);
        const msg = hours > 1 ? `我睡了${hours}个小时……你回来啦~` : "你回来啦~";
        showBubble(msg, 8000, "bubble-calm");
      }
    }).then((un) => unlisteners.push(un));

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
          let msg = "";
          if (action.action_type === "followup") {
            msg = action.message_hint + "怎么样啦？";
          } else if (action.action_type === "random_chat") {
            const greetings = [
              "你在忙什么呀？",
              "累不累？休息一下吧。",
              "我在呢，有什么事吗？",
            ];
            msg = greetings[Math.floor(Math.random() * greetings.length)];
          }
          if (msg) showBubble(msg, 12000, "bubble-calm");
        }
      } catch { /* ignore */ }
    }, 5 * 60 * 1000);

    return () => {
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
    const bounds = { width: window.innerWidth, height: window.innerHeight };
    const pos = spatialRef.current!.init(bounds);
    setPetPos(pos);
  }, []);

  // P12: Physics + spatial + circadian loop (Body layer, independent of LLM)
  useEffect(() => {
    let raf = 0;
    let lastTime = performance.now();

    const loop = (now: number) => {
      const dt = Math.min(0.05, (now - lastTime) / 1000); // cap at 50ms
      lastTime = now;

      const physics = physicsRef.current!;
      const spatial = spatialRef.current!;
      const bounds = { width: window.innerWidth, height: window.innerHeight };

      if (petPos && !isBeingDragged && !awayMode) {
        // Physics update (gravity when falling)
        const result = physics.update(petPos, dt, bounds);
        if (result.pos.y !== petPos.y || result.bounced || result.landed) {
          setPetPos(result.pos);
          if (result.landed) {
            // Landed: spatial takes over
          }
        }

        // Spatial update (walk back to nest when grounded)
        if (physics.isGrounded) {
          const interacting = isThinking || attention === AttentionState.Focused || inputVisible;
          const spatialResult = spatial.tick(result.pos, dt, interacting, true);
          if (spatialResult.isWalking) {
            setPetPos(spatialResult.newPos);
            setIsWalking(true);
          } else {
            setIsWalking(false);
          }
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
  // FIX-E / FIX-M: window-internal drag with a movement threshold so that
  // a clean click falls through to the Live2D hit detection (head/body),
  // while a move beyond ~5px starts dragging the pet within the window.
  const DRAG_THRESHOLD = 5;

  const handleDragStart = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return; // left click only
    const pos = petPos;
    dragStartRef.current = {
      x: e.clientX,
      y: e.clientY,
      offsetX: e.clientX - (pos ? pos.x : 0),
      offsetY: e.clientY - (pos ? pos.y : 0),
    };
    dragPendingRef.current = true;
  }, [petPos]);

  useEffect(() => {
    if (!dragPendingRef.current && !isBeingDragged) return;
    const onMove = (e: MouseEvent) => {
      const start = dragStartRef.current;
      if (!start) return;
      if (dragPendingRef.current && !isBeingDragged) {
        const dx = e.clientX - start.x;
        const dy = e.clientY - start.y;
        if (Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return;
        // crossed threshold -> enter real drag
        dragPendingRef.current = false;
        setIsBeingDragged(true);
        physicsRef.current?.startDrag();
      }
      if (isBeingDragged) {
        const bounds = { width: window.innerWidth, height: window.innerHeight };
        const nx = Math.max(0, Math.min(bounds.width - 400, e.clientX - start.offsetX));
        const ny = Math.max(0, Math.min(bounds.height - 600, e.clientY - start.offsetY));
        setPetPos({ x: nx, y: ny });
      }
    };
    const onUp = () => {
      dragPendingRef.current = false;
      if (isBeingDragged) {
        physicsRef.current?.release();
        setIsBeingDragged(false);
      }
      dragStartRef.current = null;
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [isBeingDragged]);

  const handleSend = useCallback(async () => {
    const text = inputText.trim();
    if (!text) { setInputVisible(false); return; }
    setInputVisible(false);
    setInputText("");
   setIsThinking(true);
   fsmRef.current?.forceState(BehaviorState.Thinking);
    thinkStartRef.current = Date.now();

   try {
     const reply = await invoke<string>("send_message", { text });
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
 }, [inputText, showBubble, moodLabel]);

  // Head pet: closeness + mood up, 3s cooldown
  const handleHeadClick = useCallback(() => {
    const now = Date.now();
    if (now - lastPetTimeRef.current < 3000) return;
    lastPetTimeRef.current = now;
    invoke("pet_head").catch(() => {});
    fsmRef.current?.transition(BehaviorState.Embarrassed);
    const reactions = ["嘿嘿…", "谢谢你～", "抹抹~"];
    showBubble(reactions[Math.floor(Math.random() * reactions.length)], 3000, "bubble-happy", "bubble-pet");
    setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 1500);
  }, [showBubble]);

 // Body poke: mood down after 3 pokes
 const handleBodyClick = useCallback(() => {
   pokeCountRef.current++;
   if (pokeResetTimerRef.current) clearTimeout(pokeResetTimerRef.current);
   pokeResetTimerRef.current = setTimeout(() => { pokeCountRef.current = 0; }, 5000);

    // FIX-C: every poke gets a visible reaction (FSM transition + graded bubble),
    // not just the 3rd. Leverages the FIX-B behavior->motion mapping on the model.
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
  }, [showBubble]);

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

      {inputVisible && (
        <div className="input-bubble">
          <input
            type="text"
            value={inputText}
            autoFocus
            placeholder={"想和我说什么？"}
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSend();
              if (e.key === "Escape") setInputVisible(false);
            }}
            onBlur={() => { if (!inputText) setInputVisible(false); }}
          />
        </div>
      )}

      <div className={`chat-bubble ${bubbleVisible ? "" : "hidden"} ${bubbleStyle} ${bubblePos}`}>
        {bubbleText}
      </div>

     <div
       ref={petRef}
        className={`pet-char-wrapper ${isWalking ? "walking" : ""} ${isBeingDragged ? "dragging" : ""}`}
        style={petPos ? { transform: `translate(${petPos.x}px, ${petPos.y}px)` } : undefined}
       onDoubleClick={() => setInputVisible(true)}
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
        />
      )}

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showDebug && <DebugPanel />}
    </div>
  );
}
