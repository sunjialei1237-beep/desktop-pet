import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DebugStandalone } from "./DebugStandalone";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles.css";

// The Debug Panel runs in its own Tauri window (open_debug_window). Query
// string (?window=debug) is NOT preserved by Tauri's release custom protocol,
// so we use the window label — every Tauri window has a unique label set at
// creation time, and getCurrentWindow().label is always available.
const isDebug = getCurrentWindow().label === "debug";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isDebug ? <DebugStandalone /> : <App />}
  </React.StrictMode>
);
