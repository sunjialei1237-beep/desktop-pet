import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { DebugPanel } from "./DebugPanel";

// Rendered when index.html loads with ?window=debug — the second Tauri window
// spawned by the open_debug_window command (F12 / Ctrl+Shift+D). The main pet
// window is only 400×760, so the Debug Panel lives out here where its native
// title bar lets the user drag it anywhere without covering Liri.
//
// close = close only this window (the pet stays running); quit = quit the app.
// AnimFSM state lives in the main window's App, so the FSM section shows a
// placeholder here — every other section (Emotion editor, Brain, Scheduler,
// Facts, …) polls the backend directly and works identically.
export function DebugStandalone() {
  return (
    <DebugPanel
      anim={{ state: "（主窗口独占）", history: [] }}
      onClose={() => getCurrentWindow().close()}
      onQuit={() => invoke("quit_app")}
    />
  );
}
