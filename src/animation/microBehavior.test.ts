import { describe, it, expect } from "vitest";
import { applySleepyWeight, IDLE_BEHAVIORS } from "./microBehavior";

// A5 effect layer: the circadian weight formula. verify-checklist's math
// (sleepiness 0.1 -> yawn ~10.7%, 0.9 -> ~32.2%, ~3x) is a consequence of this
// formula over IDLE_BEHAVIORS. We pin the formula's direction + invariants.

const yawn = IDLE_BEHAVIORS.find((b) => b.name === "yawn")!;
const lookAround = IDLE_BEHAVIORS.find((b) => b.name === "look_around")!;

describe("applySleepyWeight — daytime invariance", () => {
  it("sleepiness = 0 is a no-op for every behavior (daytime unchanged)", () => {
    for (const b of IDLE_BEHAVIORS) {
      expect(applySleepyWeight(b.weight, b.sleepy, 0)).toBeCloseTo(b.weight);
    }
  });

  it("undefined sleepy is treated as 1 (time-invariant)", () => {
    expect(applySleepyWeight(2.0, undefined, 0.9)).toBeCloseTo(2.0);
    expect(applySleepyWeight(2.0, undefined, 0.0)).toBeCloseTo(2.0);
  });
});

describe("applySleepyWeight — night-time drowsiness (A5 direction)", () => {
  it("yawn (sleepy > 1) weight rises at DeepNight vs Morning", () => {
    const morning = applySleepyWeight(yawn.weight, yawn.sleepy, 0.1);
    const deepNight = applySleepyWeight(yawn.weight, yawn.sleepy, 0.9);
    expect(deepNight).toBeGreaterThan(morning);
    // Documented ~3x uplift at night (yawn.sleepy = 5 -> big multiplier).
    expect(deepNight / morning).toBeGreaterThan(2);
  });

  it("look_around (sleepy < 1) weight drops at night (low-energy fidget fades)", () => {
    const morning = applySleepyWeight(lookAround.weight, lookAround.sleepy, 0.1);
    const deepNight = applySleepyWeight(lookAround.weight, lookAround.sleepy, 0.9);
    expect(deepNight).toBeLessThan(morning);
  });

  it("the night/day yawn ratio exceeds the look_around ratio (drowsy signals dominate)", () => {
    const yawnRatio =
      applySleepyWeight(yawn.weight, yawn.sleepy, 0.9) /
      applySleepyWeight(yawn.weight, yawn.sleepy, 0.1);
    const lookRatio =
      applySleepyWeight(lookAround.weight, lookAround.sleepy, 0.9) /
      applySleepyWeight(lookAround.weight, lookAround.sleepy, 0.1);
    expect(yawnRatio).toBeGreaterThan(lookRatio);
  });
});

describe("applySleepyWeight — clamping", () => {
  it("clamps a driven-to-zero weight to the 0.01 floor (stays pickable)", () => {
    // look_around sleepy=0.3 at full sleepiness would go very small but not vanish.
    const w = applySleepyWeight(lookAround.weight, lookAround.sleepy, 1.0);
    expect(w).toBeGreaterThanOrEqual(0.01);
  });

  it("clamps an explicit zero base to the floor", () => {
    expect(applySleepyWeight(0, 0, 1)).toBeCloseTo(0.01);
  });
});
