import { describe, it, expect } from "vitest";
import { patchLiriJson, isLiriSkeleton } from "./liriAssetPatch";

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
      { time: 3.93, name: null },
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
      { time: 3.93, name: null },
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
