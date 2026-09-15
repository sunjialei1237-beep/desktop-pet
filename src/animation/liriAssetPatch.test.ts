import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { patchLiriJson, isLiriSkeleton, stripCrossDomainPins } from "./liriAssetPatch";

// The liriAssetPatch is the ONLY thing that makes Liri's mouth correct at
// runtime (see the module doc). These tests pin the exact data transforms so a
// refactor can't silently regress the three defects: (1) mouth slots shown at
// idle, (2) smile's big-mouth deform invisible because the attachment is null
// when the deform timeline runs.
//
// A full real liri.json is ~big; the tests build a minimal skeleton that has
// JUST the structure the patch keys off (the three mouth slots, body_breath's
// null-killing timeline, smile's deform timeline) — enough to prove the patch
// does the right thing and is idempotent.

function realSkeletonFixture(): any {
  // Mirrors the relevant parts of public/spine/liri/liri.json. These are the
  // DEFECTIVE values the shipped asset has today (before the artist fix).
  return {
    skeleton: { hash: "x", spine: "3.8.75", x: 0, y: 0, width: 1, height: 1 },
    bones: [{ name: "root" }],
    slots: [
      { name: "脸", attachment: "脸" },
      // These three wrongly show at setup pose:
      { name: "嘴", attachment: "嘴" },
      { name: "小笑嘴", attachment: "小笑嘴" },
      { name: "张大笑嘴", attachment: "张大笑嘴" },
    ],
    animations: {
      // body_breath nulls 张大笑嘴 every frame (track 0, the looping base).
      body_breath: {
        slots: { 张大笑嘴: { attachment: [{ name: null }] } },
        deform: { default: {} },
      },
      // smile has the big-mouth DEFORM but no slot attachment timeline to SHOW
      // the attachment — so the deform early-returns at runtime.
      smile: {
        slots: {}, // <-- the bug: no 张大笑嘴 show/null timeline
        deform: {
          default: {
            张大笑嘴: { 张大笑嘴: [{ time: 0, vertices: [42] }, { time: 3.93, vertices: [42] }] },
            嘴: { 嘴: [{ time: 0.23, vertices: [2] }] },
          },
        },
      },
    },
  };
}

describe("isLiriSkeleton (guard)", () => {
  it("recognizes an object with bones + the three mouth slots", () => {
    expect(isLiriSkeleton(realSkeletonFixture())).toBe(true);
  });
  it("rejects null / non-objects", () => {
    expect(isLiriSkeleton(null)).toBe(false);
    expect(isLiriSkeleton("nope")).toBe(false);
    expect(isLiriSkeleton({})).toBe(false);
  });
  it("rejects a skeleton missing the mouth slots (not liri)", () => {
    const s = realSkeletonFixture();
    s.slots = [{ name: "脸", attachment: "脸" }];
    expect(isLiriSkeleton(s)).toBe(false);
  });
  it("rejects an object with slots but no bones", () => {
    const s = realSkeletonFixture();
    delete s.bones;
    expect(isLiriSkeleton(s)).toBe(false);
  });
});

describe("patchLiriJson (the fix)", () => {
  it("hides 嘴/小笑嘴/张大笑嘴 at setup pose (defect 1)", () => {
    const j = realSkeletonFixture();
    patchLiriJson(j);
    const get = (n: string) => j.slots.find((s: any) => s.name === n).attachment;
    expect(get("嘴")).toBeNull();
    expect(get("小笑嘴")).toBeNull();
    expect(get("张大笑嘴")).toBeNull();
    // 脸 (the full-face layer with closed mouth) is untouched.
    expect(get("脸")).toBe("脸");
  });

  it("adds a show→null attachment timeline for 张大笑嘴 to smile (defect 2)", () => {
    const j = realSkeletonFixture();
    patchLiriJson(j);
    const tl = j.animations.smile.slots["张大笑嘴"].attachment;
    expect(tl).toEqual([
      { time: 0, name: "张大笑嘴" },
      { time: 3.9333, name: null },
    ]);
  });

  it("leaves body_breath's null-killing timeline intact (we WANT no big mouth at idle)", () => {
    const j = realSkeletonFixture();
    patchLiriJson(j);
    expect(j.animations.body_breath.slots["张大笑嘴"]).toEqual({ attachment: [{ name: null }] });
  });

  it("returns true when it changed something, false when already patched (idempotent)", () => {
    const j = realSkeletonFixture();
    expect(patchLiriJson(j)).toBe(true); // first run fixes the defects
    expect(patchLiriJson(j)).toBe(false); // second run is a no-op
    // And the data is still correct after the second (no-op) run.
    const tl = j.animations.smile.slots["张大笑嘴"].attachment;
    expect(tl).toEqual([
      { time: 0, name: "张大笑嘴" },
      { time: 3.9333, name: null },
    ]);
  });

  it("is a no-op (returns false) for a non-liri object", () => {
    expect(patchLiriJson({ foo: 1 })).toBe(false);
    expect(patchLiriJson(null)).toBe(false);
  });

  it("does not duplicate or corrupt the timeline if smile already has other slot timelines", () => {
    const j = realSkeletonFixture();
    j.animations.smile.slots["右闭眼"] = { attachment: [{ time: 0, name: "右闭眼" }] };
    patchLiriJson(j);
    // Other slot timelines preserved.
    expect(j.animations.smile.slots["右闭眼"]).toEqual({ attachment: [{ time: 0, name: "右闭眼" }] });
    // 张大笑嘴 added correctly.
    expect(j.animations.smile.slots["张大笑嘴"].attachment[0]).toEqual({ time: 0, name: "张大笑嘴" });
  });
});

// ── Family C: cross-domain pin stripping ──
// Every secondary animation in the v2 export opens with flatline rotate pins on
// shared bones (head/spine/lh/tail_1/ear bases) equal to the breath-start pose.
// Stripping them makes concurrent channels own disjoint bone sets. These tests
// pin the strip rule: flatline-at-base goes, real animation stays.

function pinFixture(): any {
  return {
    skeleton: { spine: "3.8.75" },
    bones: [{ name: "head" }, { name: "tail_1" }, { name: "ear_l2" }, { name: "spine2" }],
    slots: [
      { name: "嘴", attachment: null },
      { name: "小笑嘴", attachment: null },
      { name: "张大笑嘴", attachment: null },
    ],
    animations: {
      // body_breath itself: lh3 is a pure pin (→ strip); spine2 is real
      // multi-keyframe sway (→ keep).
      body_breath: {
        bones: {
          lh3: { rotate: [{ time: 0, angle: -4.47 }] },
          spine2: {
            rotate: [
              { time: 0, angle: 0 },
              { time: 2, angle: 5.6 },
              { time: 4, angle: 0 },
            ],
          },
        },
      },
      // ear_idle: head pin (strip), tail_1 pin (strip), ear_l2 real swing (keep).
      ear_idle: {
        bones: {
          head: { rotate: [{ time: 0, angle: 0.57 }] },
          tail_1: { rotate: [{ time: 0, angle: 0 }] },
          ear_l2: {
            rotate: [
              { time: 0, angle: 0 },
              { time: 0.3, angle: 16.9 },
              { time: 1, angle: 0 },
            ],
          },
        },
      },
      // eye_sad-style flatline: TWO keyframes but both at base (→ strip).
      eye_sad: {
        bones: {
          head: {
            rotate: [
              { time: 0, angle: 0.57 },
              { time: 2, angle: 0.57 },
            ],
          },
        },
      },
      // A constant that is NOT the breath base is real authoring (→ keep).
      smile: {
        bones: { spine3: { rotate: [{ time: 0, angle: 99 }] } },
      },
      // Animations outside the strip list are never touched.
      future_anim: { bones: { head: { rotate: [{ time: 0, angle: 0.57 }] } } },
    },
  };
}

describe("stripCrossDomainPins (family C)", () => {
  it("strips flatline-at-base pins, keeps real timelines and non-base constants", () => {
    const j = pinFixture();
    const removed = stripCrossDomainPins(j);
    // body_breath.lh3, ear_idle.head, ear_idle.tail_1, eye_sad.head = 4
    expect(removed).toBe(4);
    expect(j.animations.body_breath.bones.lh3).toBeUndefined();
    expect(j.animations.body_breath.bones.spine2.rotate).toHaveLength(3); // real sway kept
    expect(j.animations.ear_idle.bones.head).toBeUndefined();
    expect(j.animations.ear_idle.bones.tail_1).toBeUndefined();
    expect(j.animations.ear_idle.bones.ear_l2.rotate).toHaveLength(3); // real swing kept
    expect(j.animations.eye_sad.bones.head).toBeUndefined(); // flatline two-kf pin
    expect(j.animations.smile.bones.spine3.rotate).toEqual([{ time: 0, angle: 99 }]); // non-base kept
    expect(j.animations.future_anim.bones.head).toBeDefined(); // unknown anim untouched
  });

  it("is idempotent (second run removes nothing)", () => {
    const j = pinFixture();
    stripCrossDomainPins(j);
    expect(stripCrossDomainPins(j)).toBe(0);
  });

  it("is a no-op for a non-liri object", () => {
    expect(stripCrossDomainPins(null)).toBe(0);
    expect(stripCrossDomainPins({ foo: 1 })).toBe(0);
  });
});

describe("patchLiriJson against the REAL v2 asset", () => {
  // Load the actual shipped skeleton so a future re-export that changes the
  // relevant structure fails loudly here instead of visually at runtime.
  const real = JSON.parse(
    readFileSync(resolve(process.cwd(), "public/spine/liri/liri.json"), "utf-8"),
  );

  it("applies all three families and is then idempotent", () => {
    expect(patchLiriJson(real)).toBe(true);
    expect(patchLiriJson(real)).toBe(false);
  });

  it("leaves breath's real sway but strips its lh pins (hair owns the bangs)", () => {
    expect(real.animations.body_breath.bones.spine2.rotate.length).toBeGreaterThan(1);
    expect(real.animations.body_breath.bones.lh3).toBeUndefined();
  });

  it("hands ear ownership to the ear channel (hair_idle no longer pins ears)", () => {
    expect(real.animations.hair_idle.bones.ear_l2).toBeUndefined();
    expect(real.animations.hair_idle.bones.ear_r2).toBeUndefined();
    expect(real.animations.ear_sad.bones.tail_1).toBeUndefined(); // tail owns tail_1
    expect(real.animations.tail_sad.bones.tail_1.rotate.length).toBeGreaterThan(1);
  });

  it("gesture (thing) keeps its real head tilt (not a strippable pin)", () => {
    expect(real.animations.thing.bones.head.rotate.length).toBeGreaterThan(1);
  });
});
