import { describe, it, expect } from "vitest";
import {
  PROGRAMS,
  SECONDS,
  EXPR_DURATIONS,
  IDLE_VARIANT,
  TRACK,
  createProgramRunner,
  isProgramActive,
  tickProgram,
  requestProgram,
  breathBoundary,
  triggerBehavior,
  pickIdleProgram,
} from "./spineIntent";
import { BehaviorState } from "./fsm";

// The composite-program model is the heart of the 2026-08 approved plan:
// programs start at a breath boundary, members run in parallel channels, and
// everything reverts together n boundaries later. These tests drive the runner
// against a fake spine.state and pin that choreography.

function fakeSpine() {
  const ops: Array<{ op: string; track: number; name?: string; loop?: boolean; mix?: number }> = [];
  return {
    ops,
    state: {
      setAnimation(track: number, name: string, loop: boolean) {
        ops.push({ op: "set", track, name, loop });
      },
      setEmptyAnimation(track: number, mix: number) {
        ops.push({ op: "empty", track, mix });
      },
    },
    // helper: last op on a track
    lastOn(track: number) {
      return [...ops].reverse().find((o) => o.track === track);
    },
  };
}

describe("PROGRAMS data integrity", () => {
  it("one-shot members fire inside their window (triggerAtBeat < beats)", () => {
    for (const def of Object.values(PROGRAMS)) {
      for (const m of def.members) {
        if (m.triggerAtBeat !== undefined) {
          expect(m.triggerAtBeat).toBeLessThan(def.beats);
        }
      }
    }
  });

  it("variant channels swap to known idle fallbacks when the window closes", () => {
    expect(IDLE_VARIANT.ear).toBe("ear_idle");
    expect(IDLE_VARIANT.tail).toBe("tail_idle");
  });

  it("sad is the approved 2-beat recipe: ear_sad once + tail_sad loop + sustained face", () => {
    const sad = PROGRAMS.sad;
    expect(sad.beats).toBe(2);
    const ear = sad.members.find((m) => m.channel === "ear")!;
    expect(ear.anim).toBe("ear_sad");
    expect(ear.loop).toBe(false); // self-returning one-shot, not a metronome
    expect(sad.members.some((m) => m.anim === "tail_sad")).toBe(true);
    expect(sad.members.some((m) => m.anim === "eye_sad")).toBe(true);
  });

  it("expression durations match the asset", () => {
    expect(EXPR_DURATIONS.smile).toBeCloseTo(3.9333, 4);
    expect(EXPR_DURATIONS.eye_sad).toBeCloseTo(2.0, 4);
    expect(SECONDS.breath).toBeCloseTo(4.3333, 4);
  });
});

describe("program runner lifecycle", () => {
  it("queues, starts at the boundary, reverts all channels at the closing boundary", () => {
    const spine = fakeSpine();
    const runner = createProgramRunner();
    expect(isProgramActive(runner)).toBe(false);

    expect(requestProgram("curious")).toBe(true);
    expect(requestProgram("sad")).toBe(false); // one slot only
    expect(isProgramActive(runner)).toBe(false); // still waiting for the boundary

    // Boundary 1: curious starts — both variant channels swap together.
    breathBoundary(spine, runner);
    expect(isProgramActive(runner)).toBe(true);
    expect(spine.lastOn(TRACK.ear)).toMatchObject({ name: "ear_2", loop: true });
    expect(spine.lastOn(TRACK.tail)).toMatchObject({ name: "tail_2", loop: true });

    // Boundary 2 (1-beat window closes): both revert to idle IN PARALLEL.
    const finished = breathBoundary(spine, runner);
    expect(finished).toBe("curious");
    expect(isProgramActive(runner)).toBe(false);
    expect(spine.lastOn(TRACK.ear)).toMatchObject({ name: "ear_idle", loop: true });
    expect(spine.lastOn(TRACK.tail)).toMatchObject({ name: "tail_idle", loop: true });
  });

  it("sad: fires the sad face at start, sustains it, reverts after 2 boundaries", () => {
    const spine = fakeSpine();
    const runner = createProgramRunner();
    requestProgram("sad");

    breathBoundary(spine, runner); // start
    expect(spine.lastOn(TRACK.expr)).toMatchObject({ name: "eye_sad", loop: false });
    expect(spine.lastOn(TRACK.ear)).toMatchObject({ name: "ear_sad", loop: false }); // ×1, hold
    expect(spine.lastOn(TRACK.tail)).toMatchObject({ name: "tail_sad", loop: true });

    // Wall clock passes one retrigger interval → the sad face re-fires.
    tickProgram(spine, runner, runner.activeDef, SECONDS.eyeSad + 0.1);
    const exprOps = spine.ops.filter((o) => o.track === TRACK.expr);
    expect(exprOps.filter((o) => o.name === "eye_sad")).toHaveLength(2);

    // Boundary 1: window not over (2 beats). Nothing reverts yet.
    breathBoundary(spine, runner);
    expect(isProgramActive(runner)).toBe(true);
    expect(spine.lastOn(TRACK.tail)).toMatchObject({ name: "tail_sad" });

    // Boundary 2: everything reverts together.
    expect(breathBoundary(spine, runner)).toBe("sad");
    expect(spine.lastOn(TRACK.ear)).toMatchObject({ name: "ear_idle" });
    expect(spine.lastOn(TRACK.tail)).toMatchObject({ name: "tail_idle" });
    expect(isProgramActive(runner)).toBe(false);
  });

  it("happyLong fires the second smile at beat 1", () => {
    const spine = fakeSpine();
    const runner = createProgramRunner();
    requestProgram("happyLong");
    breathBoundary(spine, runner); // smile #1 at start
    expect(spine.ops.filter((o) => o.name === "smile")).toHaveLength(1);

    tickProgram(spine, runner, runner.activeDef, SECONDS.breath + 1 / 30); // past beat 1
    expect(spine.ops.filter((o) => o.name === "smile")).toHaveLength(2);

    breathBoundary(spine, runner);
    expect(breathBoundary(spine, runner)).toBe("happyLong");
  });

  it("thing: gesture track plays once, then fades out on revert", () => {
    const spine = fakeSpine();
    const runner = createProgramRunner();
    requestProgram("thing");
    breathBoundary(spine, runner);
    expect(spine.lastOn(TRACK.gesture)).toMatchObject({ name: "thing", loop: false });
    breathBoundary(spine, runner);
    const last = spine.lastOn(TRACK.gesture)!;
    expect(last.op).toBe("empty");
    expect(last.mix).toBeGreaterThan(0);
  });

  it("stops re-firing the sad face near the window edge (rest before the boundary)", () => {
    const spine = fakeSpine();
    const runner = createProgramRunner();
    requestProgram("sad");
    breathBoundary(spine, runner);
    // Drive elapsed per-frame (like the real ticker) past every retrigger
    // interval into the last second of the window.
    for (let t = 0; t < 8.5; t += 0.016) {
      tickProgram(spine, runner, runner.activeDef, 0.016);
    }
    const fires = spine.ops.filter((o) => o.track === TRACK.expr && o.name === "eye_sad");
    // t=0 + re-fires at ~2.05 / ~4.10 / ~6.15; the ~8.20s one is refused
    // (window edge too close — she rests before the closing boundary).
    expect(fires).toHaveLength(4);
  });
});

describe("triggerBehavior serial discipline", () => {
  it("winks only when no program owns the face channel", () => {
    const spine = fakeSpine();
    expect(triggerBehavior(spine, BehaviorState.Embarrassed, false)).toBe(true);
    expect(spine.lastOn(TRACK.expr)?.name).toMatch(/^wink_[LR]$/);
    expect(triggerBehavior(spine, BehaviorState.Embarrassed, true)).toBe(false);
    expect(spine.ops.filter((o) => o.track === TRACK.expr)).toHaveLength(1);
  });
});

describe("pickIdleProgram weights", () => {
  it("only returns known program ids", () => {
    for (let i = 0; i < 200; i++) {
      expect(PROGRAMS[pickIdleProgram()]).toBeDefined();
    }
  });
});
