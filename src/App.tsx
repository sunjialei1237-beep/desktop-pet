import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export default function App() {
  const [bubbleText, setBubbleText] = useState<string>("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  const [appStatus, setAppStatus] = useState("idle");

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    listen<string>("bubble-show", (event) => {
      setBubbleText(event.payload);
      setBubbleVisible(true);
    }).then((un) => unlisteners.push(un));

    listen("bubble-hide", () => {
      setBubbleVisible(false);
    }).then((un) => unlisteners.push(un));

    listen<string>("app-status", (event) => {
      setAppStatus(event.payload);
    }).then((un) => unlisteners.push(un));

    return () => unlisteners.forEach((un) => un());
  }, []);

  const handleSend = useCallback(async () => {
    if (!inputText.trim()) {
      setInputVisible(false);
      return;
    }
    setInputVisible(false);
    setAppStatus("thinking");
    const reply = await invoke<string>("send_message", { text: inputText.trim() });
    setInputText("");
    if (reply) {
      setBubbleText(reply);
      setBubbleVisible(true);
      setAppStatus("idle");
    }
  }, [inputText]);

  return (
    <div
      className="pet-container"
      onDoubleClick={() => setInputVisible(true)}
    >
      <canvas className="pet-canvas" />

      <div className="placeholder-char">
        {appStatus === "thinking" ? "..." : ""}
      </div>

      {inputVisible && (
        <div className="input-bubble">
          <input
            type="text"
            placeholder="想和我说什么?"
            value={inputText}
            autoFocus
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSend();
              if (e.key === "Escape") setInputVisible(false);
            }}
            onBlur={() => { if (!inputText) setInputVisible(false); }}
          />
        </div>
      )}

      <div className={`chat-bubble ${bubbleVisible ? "" : "hidden"}`}>
        {bubbleText}
      </div>
    </div>
  );
}
