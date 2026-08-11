import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DebugStandalone } from "./DebugStandalone";
import "./styles.css";

// The Debug Panel runs in its own Tauri window (open_debug_window) loading the
// same index.html with ?window=debug. Branch here so the debug window renders
// just the panel instead of the whole pet app.
const isDebug =
  new URLSearchParams(window.location.search).get("window") === "debug";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isDebug ? <DebugStandalone /> : <App />}
  </React.StrictMode>
);
