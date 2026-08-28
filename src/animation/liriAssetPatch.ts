// Runtime asset patch for Liri's Spine skeleton.
//
// WHAT / WHY
// ----------
// The shipped `public/spine/liri/liri.json` ships with two families of
// data-level defects that CANNOT be fixed with per-frame code (spine.apply()
// owns every frame; a manual setAttachment is overwritten, and a DeformTimeline
// early-returns when the slot's attachment is null). Both are fixed here, at
// parse time, idempotently.
//
// Family A/B — mouth-slot defects (LiriProject v1, persists in v2):
//
//   1. `嘴` / `小笑嘴` / `张大笑嘴` have their setup-pose attachment set to a
//      shown image. Nothing in any animation hides `嘴`/`小笑嘴`, so after every
//      apply() the "unkeyed slot reset" step restores them to setup → they render
//      forever, stacked on `脸` (which already draws a closed mouth) → Liri's
//      mouth looks permanently open at idle.
//
//   2. `body_breath` keys `张大笑嘴` → null at frame 0. It is the looping base,
//      so it re-nulls the big mouth every frame.
//
//   3. The `smile` animation deforms BOTH `张大方嘴`-grade meshes (`张大笑嘴`
//      big grin 0.4→3.33s, `嘴` closed-smile morph) BUT has no slot-attachment
//      timeline to SHOW them. Because of (2), the attachments are null when the
//      deform timelines run, so DeformTimeline.apply() early-returns → no smile.
//
//   Fix A: hide 嘴/小笑嘴/张大笑嘴 at setup pose (attachment: null).
//   Fix B: give `smile` a slot-attachment timeline for 张大笑嘴: show at t=0,
//          null at t=SMILE_DURATION. Higher-track slot keys win while smile
//          plays; after it ends the null hides the big mouth again.
//
// Family C — cross-domain "pin" keyframes (composite-channel blocker):
//
//   Every secondary animation (ear_*/tail_*/hair_idle/arm_idle/Skirt_l…) opens
//   with SINGLE rotate keyframes at t=0 that pin shared bones (head / spine /
//   spine2 / spine3 / lh3 / lh4 / liuhai2 …) to the BREATH'S START VALUES —
//   the artist authored every action from the breath-neutral pose. Pinning has
//   two consequences when Liri plays in composite channels:
//     • any pinned bone freezes (spine sway dies under a looping tail), and
//     • two concurrently-playing tracks fight over the same bone (ear_sad pins
//       tail_1@0° while tail_sad swings it to 7.7°; hair_idle pins ears while
//       the ear channel wants them alive).
//   Fix C strips those FLATLINE rotate pins whose every keyframe equals the
//   known breath-start base (±0.01 tolerance, starting at t=0). Multi-value
//   timelines (real animation) and ANY constant ≠ base are kept untouched. Result: every
//   concurrently-playable track owns a DISJOINT bone set, so channel stacking
//   order stops mattering and emotion composites blend without jumps.
//
// REMOVAL
// -------
// Families A/B are runtime stand-ins for an artist fix: when the artist sets
// the three slots' setup attachment to null in Spine AND adds the show/null
// keyframes to the smile animation, A/B can be deleted. Family C goes away if
// the artist stops recording setup-pose pins into secondary animations. Until
// then this file is load-bearing; unit tests pin every transform.

// The liri.json attachment URL SpineCanvas loads. Match SpineCanvas exactly —
// patchLiriJson only touches data parsed from THIS asset.
export const LIRI_JSON_URL = "/spine/liri/liri.json";

// Slots whose setup pose wrongly shows a mouth image. Each is hidden (null) at
// setup so the unkeyed-slot reset keeps them off at idle.
const HIDE_AT_SETUP = ["嘴", "小笑嘴", "张大笑嘴"] as const;

// Exact smile length in the v2 export (`张大笑嘴` deform returns to collapsed
// at 3.9333). The show timeline's null frame MUST match the deform's final
// shrink so the big mouth disappears exactly when its mesh has collapsed.
export const SMILE_DURATION = 3.9333;

// ── Family C: the breath-start base values (exact, from v2 skeleton.json) ──
// A flatline rotate timeline (all keyframes equal to one of these, starting at
// t=0) on a NON-breath animation is a cross-domain pin → stripped. Anything
// else stays. NOTE: 0-valued pins appear in the JSON as keyframes with NO
// angle field (Spine omits defaults) — handled by the ?? 0 in the matcher.
// These are the artist's rounded setup values, not "nice" numbers — do not
// "clean them up" without re-parsing a fresh export.
const BREATH_BASE_ROTATE: Record<string, number> = {
  head: 0.57,
  lh3: -4.47,
  lh4: 0,
  liuhai2: 3.95,
  spine: 0.54,
  spine2: 0,
  spine3: -0.03,
  tail_1: 0,
  ear_l2: 0,
  ear_r2: -12.54,
};
const PIN_TOLERANCE = 0.01;

// Animations eligible for pin-stripping. Deliberately NOT an allowlist-per-bone:
// the generic rule (exactly ONE keyframe, at t=0, angle ≈ base) can't hurt a
// real animation, which always has multiple keyframes. `body_breath` is excluded
// wholesale — its own single-frame pins on lh3/lh4/liuhai2 must ALSO go (hair_idle
// owns the bangs swing below it), but its head/spine timelines are multi-kf and
// survive on merit.
const PIN_STRIP_ANIMS = new Set([
  "arm_idle",
  "hair_idle",
  "Skirt_l",
  "ear_idle",
  "ear_2",
  "ear_sad",
  "tail_idle",
  "tail_2",
  "tail_happy",
  "tail_sad",
  "eye_sad",
  "blink",
  "wink_L",
  "wink_R",
  "smile",
  "body_breath",
]);

// Detect a raw parsed liri.json object. Spine skeletons always have `bones` and
// `slots`; the liri asset additionally carries the three mouth slots we patch.
// Returning false on a non-matching object keeps the patch a safe no-op for any
// other JSON that happens to pass through.
export function isLiriSkeleton(obj: unknown): boolean {
  if (!obj || typeof obj !== "object") return false;
  const o = obj as Record<string, unknown>;
  if (!Array.isArray(o.slots) || !o.bones || typeof o.bones === "undefined") return false;
  const slots = o.slots as Array<{ name?: unknown }>;
  const names = new Set(slots.map((s) => s.name));
  return HIDE_AT_SETUP.every((n) => names.has(n));
}

function isBreathBasePin(tl: Array<{ time?: number; angle?: number }>, base: number): boolean {
  // Starts at t=0 and NEVER leaves the base value — a flatline at the breath
  // start pose. Covers both the common single-kf pin and flatline variants
// like eye_sad's two identical head keyframes.
  return (
    tl.length > 0 &&
    Math.abs((tl[0].time ?? 0) - 0) < 1e-6 &&
    tl.every((kf) => Math.abs((kf.angle ?? 0) - base) <= PIN_TOLERANCE)
  );
}

/// Family C: remove cross-domain single-frame t=0 rotate pins. Idempotent.
/// Returns the number of removed bone timelines (for a one-time dev log/test).
export function stripCrossDomainPins(obj: unknown): number {
  if (!isLiriSkeleton(obj)) return 0;
  const o = obj as Record<string, any>;
  let removed = 0;
  for (const [name, anim] of Object.entries((o.animations ?? {}) as Record<string, any>)) {
    if (!PIN_STRIP_ANIMS.has(name)) continue;
    const bones = anim?.bones;
    if (!bones || typeof bones !== "object") continue;
    for (const [bone, channels] of Object.entries(bones)) {
      const base = BREATH_BASE_ROTATE[bone];
      const rotate = (channels as Record<string, unknown>)?.rotate;
      if (
        base === undefined ||
        !Array.isArray(rotate) ||
        !isBreathBasePin(rotate as Array<{ time?: number; angle?: number }>, base)
      ) {
        continue;
      }
      delete (channels as Record<string, unknown>).rotate;
      if (Object.keys(channels as object).length === 0) delete bones[bone];
      removed++;
    }
  }
  return removed;
}

// Idempotent: safe to call on an already-patched object. Returns true if it
// changed anything (useful for a one-time dev log), false if already patched.
export function patchLiriJson(obj: unknown): boolean {
  if (!isLiriSkeleton(obj)) return false;
  const o = obj as Record<string, any>;
  let changed = false;

  // Fix A: Hide the three mouth slots at setup pose.
  for (const slot of o.slots as Array<{ name: string; attachment?: unknown }>) {
    if (HIDE_AT_SETUP.includes(slot.name as (typeof HIDE_AT_SETUP)[number])) {
      if (slot.attachment !== null && slot.attachment !== undefined) {
        slot.attachment = null;
        changed = true;
      }
    }
  }

  // Fix B: Add a show→null attachment timeline to the smile animation for 张大笑嘴.
  const smile = o.animations?.smile;
  if (smile && typeof smile === "object") {
    smile.slots = smile.slots ?? {};
    const existing = smile.slots["张大笑嘴"]?.attachment;
    const want = [
      { time: 0, name: "张大笑嘴" },
      { time: SMILE_DURATION, name: null },
    ];
    // Only write if absent or different (idempotent).
    const same =
      Array.isArray(existing) &&
      existing.length === want.length &&
      existing.every(
        (f: { time?: number; name?: unknown }, i: number) =>
          f.time === want[i].time && f.name === want[i].name,
      );
    if (!same) {
      smile.slots["张大笑嘴"] = { attachment: want };
      changed = true;
    }
  }

  // Fix C: strip cross-domain pins so composite channels never fight/freeze.
  if (stripCrossDomainPins(o) > 0) changed = true;

  return changed;
}
