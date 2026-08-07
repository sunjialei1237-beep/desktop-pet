import { describe, it, expect } from "vitest";
import { shouldAutoSleep, type AutoSleepInput } from "./sleepLogic";
import { TimeOfDay } from "./circadian";
import { BehaviorState } from "./fsm";

// A4 trigger layer: the DeepNight drift-off condition. Each branch of the
// conjunction is pinned so a future refactor can't silently keep her awake
// (or, worse, put her to sleep mid-conversation).

const THRESH = 10 * 60 * 1000; // SLEEP_AFTER_IDLE_MS

/** A baseline that SHOULD trigger sleep; each test flips one field to break it. */
function baseline(over: Partial<AutoSleepInput> = {}): AutoSleepInput {
  return {
    period: TimeOfDay.DeepNight,
    state: BehaviorState.Idle,
    isThinking: false,
    isTalking: false,
    idleMs: THRESH + 1, // strictly past the threshold
    thresholdMs: THRESH,
    ...over,
  };
}

describe("shouldAutoSleep (A4 trigger)", () => {
  it("sleeps when DeepNight + idle + quiet + past threshold", () => {
    expect(shouldAutoSleep(baseline())).toBe(true);
  });

  it("does NOT sleep outside DeepNight (LateNight only yawns)", () => {
    expect(shouldAutoSleep(baseline({ period: TimeOfDay.LateNight }))).toBe(false);
    expect(shouldAutoSleep(baseline({ period: TimeOfDay.Morning }))).toBe(false);
    expect(shouldAutoSleep(baseline({ period: TimeOfDay.Evening }))).toBe(false);
  });

  it("does NOT re-trigger once already sleeping (idempotent entry)", () => {
    expect(
      shouldAutoSleep(baseline({ state: BehaviorState.Sleeping })),
    ).toBe(false);
  });

  it("does NOT sleep while thinking (LLM reply pending)", () => {
    expect(shouldAutoSleep(baseline({ isThinking: true }))).toBe(false);
  });

  it("does NOT sleep while talking (reply streaming)", () => {
    expect(shouldAutoSleep(baseline({ isTalking: true }))).toBe(false);
  });

  it("does NOT sleep before the idle threshold (strict >)", () => {
    expect(shouldAutoSleep(baseline({ idleMs: THRESH }))).toBe(false); // exactly at threshold
    expect(shouldAutoSleep(baseline({ idleMs: THRESH - 1 }))).toBe(false); // just under
  });

  it("a fresh interaction (idle reset) keeps her awake even in DeepNight", () => {
    // Simulates the moment right after markInteraction() refreshes the clock.
    expect(shouldAutoSleep(baseline({ idleMs: 100 }))).toBe(false);
  });
});
