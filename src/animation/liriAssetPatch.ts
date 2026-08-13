// Runtime asset patch for Liri's Spine skeleton.
//
// WHAT / WHY
// ----------
// The shipped `public/spine/liri/liri.json` has three mouth-slot defects that
// make the face wrong at runtime, and they CANNOT be fixed with per-frame code
// (spine.apply() owns every frame; a manual setAttachment is overwritten, and
// — crucially — a DeformTimeline early-returns when the slot's attachment is
// null, so a post-update hook is too late). The defects:
//
//   1. `嘴` / `小笑嘴` / `张大笑嘴` have their setup-pose attachment set to a
//      shown image. Nothing in any animation hides `嘴`/`小笑嘴`, so after every
//      apply() the "unkeyed slot reset" step restores them to setup → they render
//      forever, stacked on `脸` (which already draws a closed mouth) → Liri's
//      mouth looks permanently open at idle.
//
//   2. `body_breath` (track 0) has a slot-attachment timeline that keys
//      `张大笑嘴` → null at frame 0. track 0 is the continuously-looping base, so
//      it re-nulls `张大笑嘴` every frame.
//
//   3. The `smile` animation has a DeformTimeline for `张大笑嘴` (the big-smile
//      mesh: shrink at t=0, expand to a big grin by t=0.4, shrink back, gone by
//      t=3.93) BUT no slot-attachment timeline to show the attachment. Because of
//      (2), the attachment is null when the deform timeline runs, so
//      DeformTimeline.apply() early-returns (Animation.js: slot.getAttachment()
//      is not a VertexAttachment) → the big smile is invisible.
//
// THE FIX (data-level, idempotent, safe to re-run)
// ------------------------------------------------
//   A. Hide 嘴/小笑嘴/张大笑嘴 at setup pose (attachment: null). The unkeyed-slot
//      reset then keeps them hidden at idle. (2) still nulls 张大笑嘴 at idle —
//      fine, we WANT no big-mouth at idle.
//   B. Give the `smile` animation a slot-attachment timeline for 张大笑嘴:
//        t=0    show '张大笑嘴'
//        t=3.93 null
//      Because timelines are parsed in slot→deform order and applied in that
//      same order within one track pass, the show runs BEFORE the deform in the
//      same track 5 pass → slot.getAttachment() is the mesh when the deform
//      runs → the deform applies. track 0's null (lower track) is overwritten by
//      track 5's show (higher track) while smile plays, and after the smile the
//      t=3.93 null hides it again.
//
// REMOVAL
// -------
// This is a runtime stand-in for an artist fix. When the artist sets the three
// slots' setup attachment to null in Spine AND adds the show/null keyframes to
// the smile animation, delete this file and its caller in SpineCanvas.tsx.

// The liri.json attachment URL SpineCanvas loads. Match SpineCanvas exactly —
// patchLiriJson only touches data parsed from THIS asset.
export const LIRI_JSON_URL = "/spine/liri/liri.json";

// Slots whose setup pose wrongly shows a mouth image. Each is hidden (null) at
// setup so the unkeyed-slot reset keeps them off at idle.
const HIDE_AT_SETUP = ["嘴", "小笑嘴", "张大笑嘴"] as const;

// Deform keyframe times on smile's `张大笑嘴` (read from the asset). The show
// timeline's null frame MUST match the deform's final shrink so the big mouth
// disappears exactly when its mesh has collapsed back to nothing.
const SMILE_DURATION = 3.93;

// Detect a raw parsed liri.json object. Spine skeletons always have `bones` and
// `slots`; the liri asset additionally has the three mouth slots + body_breath +
// smile animations we patch. Returning false on a non-matching object keeps the
// patch a safe no-op for any other JSON that happens to pass through.
export function isLiriSkeleton(obj: unknown): boolean {
  if (!obj || typeof obj !== "object") return false;
  const o = obj as Record<string, unknown>;
  if (!Array.isArray(o.slots) || !o.bones || typeof o.bones !== "object") return false;
  const slots = o.slots as Array<{ name?: unknown }>;
  const names = new Set(slots.map((s) => s.name));
  return HIDE_AT_SETUP.every((n) => names.has(n));
}

// Idempotent: safe to call on an already-patched object (re-running detects the
// smile attachment timeline and leaves it). Returns true if it changed anything
// (useful for a one-time dev log), false if the data already looked fixed.
export function patchLiriJson(obj: unknown): boolean {
  if (!isLiriSkeleton(obj)) return false;
  const o = obj as Record<string, any>;
  let changed = false;

  // (A) Hide the three mouth slots at setup pose.
  for (const slot of o.slots as Array<{ name: string; attachment?: unknown }>) {
    if (HIDE_AT_SETUP.includes(slot.name as (typeof HIDE_AT_SETUP)[number])) {
      if (slot.attachment !== null && slot.attachment !== undefined) {
        slot.attachment = null;
        changed = true;
      }
    }
  }

  // (B) Add a show→null attachment timeline to the smile animation for 张大笑嘴.
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

  return changed;
}
