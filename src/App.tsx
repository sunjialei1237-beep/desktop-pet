import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { PetCharacter } from "./PetCharacter";
import { SettingsPanel } from "./SettingsPanel";
import { DebugPanel } from "./DebugPanel";

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

export default function App() {
  const [bubbleText, setBubbleText] = useState("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  const [isThinking, setIsThinking] = useState(false);
  const [moodLabel, setMoodLabel] = useState("ping jing");
  const [showSettings, setShowSettings] = useState(false);
  const [showDebug, setShowDebug] = useState(false);
  const bubbleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showBubble = useCallback((text: string, duration = 8000) => {
    setBubbleText(text);
    setBubbleVisible(true);
    if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
    bubbleTimerRef.current = setTimeout(() => setBubbleVisible(false), duration);
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
      showBubble(event.payload.title + " zenmeyang la?", 15000);
    }).then((un) => unlisteners.push(un));

    const emotionTimer = setInterval(async () => {
      try {
        const emo = await invoke<EmotionData>("get_emotion_state");
        setMoodLabel(emo.mood_label);
      } catch {
        // ignore
      }
    }, 5000);

    invoke<EmotionData>("get_emotion_state")
      .then((emo) => setMoodLabel(emo.mood_label))
      .catch(() => {});

    const proactiveTimer = setInterval(async () => {
      try {
        const action = await invoke<ProactiveAction | null>("check_proactive");
        if (action) {
          let msg = "";
          if (action.action_type === "followup") {
            msg = action.message_hint + " zenmeyang la?";
          } else if (action.action_type === "random_chat") {
            const greetings = [
              "ni zai mang shenme ya?",
              "lei bu lei? xiu xi yixia ba.",
              "wo zai ne, you shenme shi ma?",
            ];
            msg = greetings[Math.floor(Math.random() * greetings.length)];
          }
          if (msg) showBubble(msg, 12000);
        }
      } catch {
        // ignore
      }
    }, 5 * 60 * 1000);

    return () => {
      unlisteners.forEach((un) => un());
      clearInterval(emotionTimer);
      clearInterval(proactiveTimer);
      if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
    };
  }, [showBubble]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F12") { e.preventDefault(); setShowDebug((v) => !v); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const handleSend = useCallback(async () => {
    const text = inputText.trim();
    if (!text) {
      setInputVisible(false);
      return;
    }
    setInputVisible(false);
    setInputText("");
    setIsThinking(true);

    try {
      const reply = await invoke<string>("send_message", { text });
      setIsThinking(false);
      if (reply) {
        showBubble(reply);
      } else {
        showBubble("...", 3000);
      }
    } catch (e) {
      setIsThinking(false);
      const errMsg = String(e);
      if (errMsg.includes("not configured") || errMsg.includes("NotConfigured")) {
        showBubble("(hai mei you peizhi hao lian jie...)", 5000);
      } else if (errMsg.includes("Timeout") || errMsg.includes("timeout")) {
        showBubble("wo...ganggang you dian zou shen...", 5000);
      } else if (errMsg.includes("Network") || errMsg.includes("network")) {
        showBubble("xin hao bu tai hao ne...", 5000);
      } else if (errMsg.includes("RateLimit") || errMsg.includes("429")) {
        showBubble("shuo le hao duo hua, rang wo chuan kou qi ba~", 5000);
      } else {
        showBubble("...", 3000);
      }
    }
  }, [inputText, showBubble]);

  return (
    <div className="pet-container">
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
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSend();
              if (e.key === "Escape") setInputVisible(false);
            }}
            onBlur={() => {
              if (!inputText) setInputVisible(false);
            }}
          />
        </div>
      )}

      <div className={`chat-bubble ${bubbleVisible ? "" : "hidden"}`}>
        {bubbleText}
      </div>

      <div
        className="pet-char-wrapper"
        onDoubleClick={() => setInputVisible(true)}
        onClick={() => invoke("poke").catch(() => {})}
      >
        <PetCharacter moodLabel={moodLabel} isThinking={isThinking} />
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

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
      {showDebug && <DebugPanel />}
    </div>
  );
}
