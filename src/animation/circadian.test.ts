import { describe, it, expect } from "vitest";
import { getCircadianState, TimeOfDay } from "./circadian";

// A5 input layer: the hour -> period/sleepiness map that drives the whole
// night-time behavior chain. These are the exact values the runtime verify
// checklist asserts via window.__pet.setHour(); pinning them as unit tests
// gives a deterministic regression net without touching the OS clock.

describe("getCircadianState — period mapping", () => {
  it("Morning = 6..10", () => {
    for (const h of [6, 8, 10]) {
      expect(getCircadianState(h).period).toBe(TimeOfDay.Morning);
    }
  });
  it("Afternoon = 11..16", () => {
    for (const h of [11, 13, 16]) {
      expect(getCircadianState(h).period).toBe(TimeOfDay.Afternoon);
    }
  });
  it("Evening = 17..21", () => {
    for (const h of [17, 19, 21]) {
      expect(getCircadianState(h).period).toBe(TimeOfDay.Evening);
    }
  });
  it("LateNight = 22..23 and 0..1 (does NOT auto-sleep, only yawns)", () => {
    for (const h of [22, 23, 0, 1]) {
      expect(getCircadianState(h).period).toBe(TimeOfDay.LateNight);
    }
  });
  it("DeepNight = 2..5 (the only auto-sleep window)", () => {
    for (const h of [2, 3, 5]) {
      expect(getCircadianState(h).period).toBe(TimeOfDay.DeepNight);
    }
  });

  it("boundaries hand off at the right hour", () => {
    // 6 -> Morning, 11 -> Afternoon, 17 -> Evening, 22 -> LateNight, 2 -> DeepNight
    expect(getCircadianState(6).period).toBe(TimeOfDay.Morning);
    expect(getCircadianState(11).period).toBe(TimeOfDay.Afternoon);
    expect(getCircadianState(17).period).toBe(TimeOfDay.Evening);
    expect(getCircadianState(22).period).toBe(TimeOfDay.LateNight);
    expect(getCircadianState(2).period).toBe(TimeOfDay.DeepNight);
  });
});

describe("getCircadianState — sleepiness (A5)", () => {
  it("DeepNight sleepiness = 0.9 (documented verify value)", () => {
    expect(getCircadianState(3).sleepiness).toBeCloseTo(0.9);
  });
  it("Morning sleepiness = 0.1 (documented verify value)", () => {
    expect(getCircadianState(10).sleepiness).toBeCloseTo(0.1);
  });
  it("sleepiness is monotonically higher at night than by day", () => {
    const day = Math.max(
      getCircadianState(8).sleepiness,
      getCircadianState(13).sleepiness,
    );
    const night = Math.max(
      getCircadianState(23).sleepiness,
      getCircadianState(3).sleepiness,
    );
    expect(night).toBeGreaterThan(day);
  });
  it("exposes speed/energy modifiers that drop at night (Body slows down)", () => {
    const day = getCircadianState(8);
    const deep = getCircadianState(3);
    expect(deep.speedModifier).toBeLessThan(day.speedModifier);
    expect(deep.energyModifier).toBeLessThan(day.energyModifier);
  });
});
