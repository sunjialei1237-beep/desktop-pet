import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { PetCharacter } from "./PetCharacter";
import { SettingsPanel } from "./SettingsPanel";
import { DebugPanel } from "./DebugPanel";
import { ContextMenu } from "./ContextMenu";
import { AnimationFSM, BehaviorState } from "./animation/fsm";
import { pickNextBehavior } from "./animation/microBehavior";
import { AttentionState, computeAttention, computeHeadAngle, type PetRect } from "./animation/attention";

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
  if (label === "第难") return "bubble-sad";
  return "bubble-calm";
}

export default function App() {
  const [bubbleText, setBubbleText] = useState("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  const [bubbleStyle, setBubbleStyle] = useState("");
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  const [isThinking, setIsThinking] = useState(false);
  const [moodLabel, setMoodLabel] = useState("平静");
  const [showSettings, setShowSettings] = useState(false);
  const [showDebug, setShowDebug] = useState(false);
  const [attention, setAttention] = useState(AttentionState.Ignored);
  const [headAngle, setHeadAngle] = useState({ angleX: 0, angleY: 0 });
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const [awayMode, setAwayMode] = useState(false);
  const bubbleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [behavior, setBehavior] = useState<BehaviorState>(BehaviorState.Idle);
  const fsmRef = useRef<AnimationFSM | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const pokeCountRef = useRef(0);
  const pokeResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPetTimeRef = useRef(0);
  const closenessRef = useRef(0);
  const moodRef = useRef(0.5);
  const energyRef = useRef(0.7);

  if (!fsmRef.current) {
    fsmRef.current = new AnimationFSM();
    fsmRef.current.onStateChange((s) => setBehavior(s));
  }

  const showBubble = useCallback((text: string, duration = 8000, style = "") => {
    setBubbleText(text);
    setBubbleStyle(style);
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
      const rect = el.getBoundingClientRect();
      const petRect: PetRect = {
        centerX: rect.left + rect.width / 2,
        centerY: rect.top + rect.height / 2,
        width: rect.width,
        height: rect.height,
      };
      setAttention(computeAttention(e.clientX, e.clientY, petRect));
      setHeadAngle(computeHeadAngle(e.clientX, e.clientY, petRect));
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
      showBubble(event.payload);
    }).then((un) => unlisteners.push(un));

    listen("bubble-hide", () => {
      setBubbleVisible(false);
    }).then((un) => unlisteners.push(un));

    listen<EmotionData>("emotion-update", (event) => {
      setMoodLabel(event.payload.mood_label);
    }).then((un) => unlisteners.push(un));

    listen<{ title: string; event_id: string }>("proactive-prompt", (event) => {
      showBubble(event.payload.title + "怎么样啦？", 15000);
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

    const proactiveTimer = setInterval(async () => {
      if (awayMode) return;
      try {
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
          if (msg) showBubble(msg, 12000);
        }
      } catch { /* ignore */ }
    }, 5 * 60 * 1000);

    return () => {
      unlisteners.forEach((un) => un());
      clearInterval(emotionTimer);
      clearInterval(proactiveTimer);
      if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
    };
  }, [showBubble, awayMode]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F12") { e.preventDefault(); setShowDebug((v) => !v); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const handleSend = useCallback(async () => {
    const text = inputText.trim();
    if (!text) { setInputVisible(false); return; }
    setInputVisible(false);
    setInputText("");
    setIsThinking(true);
    fsmRef.current?.forceState(BehaviorState.Thinking);

    try {
      const reply = await invoke<string>("send_message", { text });
      setIsThinking(false);
      fsmRef.current?.forceState(BehaviorState.Talking);
      if (reply) {
        showBubble(reply, 10000, bubbleClassForMood(moodLabel));
      } else {
        showBubble("...", 3000);
      }
      setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 2000);
    } catch (e) {
      setIsThinking(false);
      fsmRef.current?.forceState(BehaviorState.Idle);
      const errMsg = String(e);
      if (errMsg.includes("not configured") || errMsg.includes("NotConfigured")) {
        showBubble("（还没有配置好连接……）", 5000);
      } else if (errMsg.includes("Timeout") || errMsg.includes("timeout")) {
        showBubble("我……刚刚有点走神……", 5000);
      } else if (errMsg.includes("Network") || errMsg.includes("network")) {
        showBubble("信号不太好呢……", 5000);
      } else if (errMsg.includes("RateLimit") || errMsg.includes("429")) {
        showBubble("说了好多话，让我喘口气吧~", 5000);
      } else {
        showBubble("...", 3000);
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
    showBubble(reactions[Math.floor(Math.random() * reactions.length)], 3000);
    setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 1500);
  }, [showBubble]);

  // Body poke: mood down after 3 pokes
  const handleBodyClick = useCallback(() => {
    pokeCountRef.current++;
    if (pokeResetTimerRef.current) clearTimeout(pokeResetTimerRef.current);
    pokeResetTimerRef.current = setTimeout(() => { pokeCountRef.current = 0; }, 5000);

    invoke<boolean>("poke", { count: pokeCountRef.current }).then((isAngry) => {
      if (isAngry) {
        showBubble("…！别戳了啦！", 3000);
      }
    }).catch(() => {});
  }, [showBubble]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  const handleExportMemory = useCallback(() => {
    showBubble("记忆导出中…", 3000);
    invoke("get_debug_snapshot").then((snap) => {
      const blob = new Blob([JSON.stringify(snap, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "pet-memory-export.json";
      a.click();
      URL.revokeObjectURL(url);
    }).catch(() => {});
  }, [showBubble]);

  const handleAwayMode = useCallback(() => {
    setAwayMode(true);
    showBubble("我先休息一下哦~", 4000);
  }, [showBubble]);

  const handleQuit = useCallback(() => {
    showBubble("再见…", 3000);
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

      <div className={`chat-bubble ${bubbleVisible ? "" : "hidden"} ${bubbleStyle}`}>
        {bubbleText}
      </div>

      <div
        ref={petRef}
        className="pet-char-wrapper"
        onDoubleClick={() => setInputVisible(true)}
      >
        <PetCharacter
          moodLabel={moodLabel}
          isThinking={isThinking}
          behavior={behavior}
          attention={attention}
          headAngleX={headAngle.angleX}
          headAngleY={headAngle.angleY}
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
