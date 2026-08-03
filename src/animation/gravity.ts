// Gravity Physics: free-fall to a hover point (design doc 6.9, MVP).
// Pure Body-layer math — no LLM, no FSM dependency (Principle 1/5).
// Window-based model (方案 B): the pet IS the OS window, so physics moves
// the window's top-left corner. `bottom` = window bottom edge.
//
// The pet never hits the taskbar: the caller arms a fall limit at 1/3 of the
// drop distance (user preference 08-01), so she floats to a stop mid-air.
// This module only integrates gravity; the stop condition lives in App.tsx.

export interface GravityState {
  vy: number; // vertical velocity in px/s, positive = downward
  grounded: boolean;
}

/// px/s^2 — user preference 08-01: fall time x3 (t ~ 1/sqrt(g), so g/9).
/// 1200 was a snappy drop; 133 is a slow, floaty descent.
export const GRAVITY = 1200 / 9;

export function createGravity(): GravityState {
  return { vy: 0, grounded: true };
}

/// One physics step. Advances velocity/position for `dt` seconds and returns
/// the new bottom edge. Floor collision is NOT handled here — the caller
/// checks a fall limit and stops her at the hover point (see App.tsx).
export function stepGravity(g: GravityState, dt: number, bottom: number): number {
  if (g.grounded) return bottom;
  g.vy += GRAVITY * dt;
  return bottom + g.vy * dt;
}
