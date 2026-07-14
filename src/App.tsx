import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { PetCharacter } from "./PetCharacter";

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
      if (errMsg.includes("not configured") || errMsg.includes("API key")) {
        showBubble("(LLM wei peizhi)", 5000);
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
    </div>
  );
}
