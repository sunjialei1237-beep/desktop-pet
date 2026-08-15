//! Global cursor tracking for click-through (ADR Phase 2).
//!
//! Polls the physical screen cursor position via Win32 GetCursorPos and emits a
//! `global-cursor` event whenever it moves beyond a small delta. The frontend
//! uses this (combined with the window origin + model bounds) to toggle
//! setIgnoreCursorEvents so clicks pass through transparent regions to the desktop.
//!
//! Design doc / ADR: GetCursorPos polling is chosen over a low-level mouse hook —
//! hooks require a message pump and are killed by Windows on timeout; polling is
//! simple, safe, and sufficient since we only need the screen coordinate.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Emitted cursor payload (physical screen pixels).
/// `lbutton` is the physical left-button state — the OS truth the frontend uses
/// to distinguish "user paused mid-drag (button held)" from "user released",
/// which movement-quiescence alone cannot (native drags swallow webview
/// mouseup, so the page never sees the release).
#[derive(Debug, Clone, Serialize)]
pub struct CursorPos {
    pub x: i32,
    pub y: i32,
    pub lbutton: bool,
}

const POLL_INTERVAL_MS: u64 = 16; // ~60Hz
const MIN_DELTA_PX: i32 = 1; // only emit on real movement

/// Starts the cursor polling thread. Returns a stop flag; the caller should set
/// it to true on app exit. The thread owns an AppHandle clone and emits events.
pub fn start(app: AppHandle) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    std::thread::spawn(move || {
        log::info!("cursor poll thread started");
        let mut last_x: i32 = i32::MIN;
        let mut last_y: i32 = i32::MIN;
        let mut last_lbutton = false;
        while !stop_clone.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
            let mut pt = windows::Win32::Foundation::POINT { x: 0, y: 0 };
            // SAFETY: GetCursorPos writes into our local POINT; no global state mutated.
            let ok = unsafe { GetCursorPos(&mut pt) }.is_ok();
            if !ok {
                continue; // rare failure, skip this tick
            }
            // SAFETY: GetAsyncKeyState reads physical button state regardless of
            // focus/mouse-capture — correct even mid native-drag modal loop.
            let lbutton = (unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } as u16 & 0x8000) != 0;
            let dx = (pt.x as i64 - last_x as i64).abs();
            let dy = (pt.y as i64 - last_y as i64).abs();
            // Emit on position change OR button-state change: a release with the
            // mouse held perfectly still still changes `lbutton`, and without
            // that event the frontend would never un-freeze the fall physics.
            if dx <= MIN_DELTA_PX as i64 && dy <= MIN_DELTA_PX as i64 && lbutton == last_lbutton {
                continue;
            }
            last_x = pt.x;
            last_y = pt.y;
            last_lbutton = lbutton;
            let _ = app.emit("global-cursor", CursorPos { x: pt.x, y: pt.y, lbutton });
        }
        log::info!("cursor poll thread stopped");
    });
    stop
}
