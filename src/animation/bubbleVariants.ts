// Bubble animation variants — the single source of truth for how a bubble
// enters, sits, and exits. Driven by Motion (AnimatePresence + motion.div).
//
// Core philosophy (per design review): all bubbles are ONE organism speaking
// in different emotional states — NOT a gallery of different UI animations.
// So there is ONE base animation (opacity + y + tiny scale), and each emotion
// only tweaks parameters (duration / easing / spring / opacity curve). Every
// emotion shares the same hidden start point (y=6, scale=0.985); no emotion
// moves the start position dramatically (sad does NOT start from y=10).
//
// Rule (design review v3 #1): variants are produced by a BUILDER, never by
// shallow-merging { ...BASE, ...EMOTION_MOD } — that would let an emotion's
// `visible` drop the base's opacity/scale and silently break the animation.
// createBubbleVariants() composes a COMPLETE variant per emotion.

import type { Variants, Transition, TargetAndTransition } from "motion/react";

// External API: the style strings the rest of the app already passes around
// (bubbleClassForMood, showBubble text/duration/style calls). Keeping these
// avoids rewiring every call site — the Bubble component translates them.
export type BubbleEmotion =
  | "bubble-happy"
  | "bubble-playful"
  | "bubble-calm"
  | "bubble-sad"
  | "bubble-worried"
  | "bubble-shy"
  | "bubble-glyph";

// Glyph sub-kind — the dot ("···") is itself a glyph variant, NOT a fixed
// prefix on every glyph (design review v3 #6). thinking shows dots; sigh gets
// a single leading dot; surprise/sleepy have none. This keeps glyphs reading
// as wordless life-signals rather than designed UI chips.
export type GlyphKind = "thinking" | "sigh" | "surprise" | "sleepy";

// --- Base shape (shared by every emotion) ---
// Converged scales (design review v2 #4): no big "pop" — life-feel comes from
// speed + micro-elasticity, not large amplitude. Avoids the "UI sticker" look.
// NEVER animate height/scaleY — streaming text grows the box every 22-72ms and
// a height/scaleY animation would compound into visible jitter.
// Values tuned so the motion is clearly perceptible (life-feel) but not bouncy
// or "UI popup": y=12 slides up visibly, scale 0.96→1 gives a soft settle.
const BASE_HIDDEN = { opacity: 0, y: 12, scale: 0.96 };
// Exit is deliberately SHORT (120-160ms) so that on rapid replacement (A→B)
// the two bubbles barely overlap visually (design review v3 #4). We keep
// AnimatePresence in default sync mode — mode="wait" would queue up back-to-
// back short bubbles and feel sluggish.
const BASE_EXIT = { opacity: 0, y: 12, scale: 0.98 };

// --- Emotion modulations (parameters ONLY, not full variant states) ---
// Each entry tweaks the visible-state transition; hidden/exit stay shared.
interface EmotionMod {
  visibleTransition: Transition;
  // Optional tiny visible-offset to differentiate, kept SMALL (no big y moves).
  visibleY?: number;
  // shy reveals in two opacity steps (0 → 0.4 → 1) for a tentative feel.
  visibleOpacitySteps?: number[];
}

const EMOTION_MODS: Record<Exclude<BubbleEmotion, "bubble-glyph">, EmotionMod> = {
  "bubble-happy": {
    // Spring with low-ish damping so the bubble gently overshoots and settles —
    // a visible "life" bounce, not a stiff stop. Stiffness 180 + damping 12 gives
    // one soft oscillation before rest.
    visibleTransition: { type: "spring", stiffness: 180, damping: 12 },
  },
  "bubble-playful": {
    // slightly bouncier than happy, still micro
    visibleTransition: { type: "spring", stiffness: 200, damping: 11 },
  },
  "bubble-calm": {
    visibleTransition: { duration: 0.4, ease: "easeOut" },
  },
  "bubble-sad": {
    // slower + settles a hair lower — but still from the SAME hidden start.
    visibleTransition: { duration: 0.55, ease: "easeOut" },
    visibleY: 1,
  },
  "bubble-worried": {
    // a very subtle rotational tremor on settle (±0.4deg) — barely perceptible.
    visibleTransition: { duration: 0.4, ease: "easeOut" },
  },
  "bubble-shy": {
    // slow reveal: opacity ramps through a half-transparent beat first.
    visibleTransition: { duration: 1.2, ease: "easeOut" },
    visibleOpacitySteps: [0, 0.4, 1],
  },
};

const GLYPH_MOD: EmotionMod = {
  // glyph is wordless — the softest, quickest fade-in.
  visibleTransition: { duration: 0.45, ease: "easeOut" },
  visibleOpacitySteps: [0, 0.8],
};

// --- The builder (the ONLY sanctioned way to get variants) ---
// Composes a COMPLETE Variant object per emotion — hidden + visible + exit all
// populated, so nothing gets dropped by a shallow merge.
export function createBubbleVariants(emotion: BubbleEmotion): Variants {
  const mod = emotion === "bubble-glyph" ? GLYPH_MOD : EMOTION_MODS[emotion];

  // Build the visible state WITHOUT mutating a shared base (each call fresh).
  const visibleTarget: TargetAndTransition = {
    opacity: (mod.visibleOpacitySteps ?? 1) as number,
    y: mod.visibleY ?? 0,
    scale: 1,
    transition: mod.visibleTransition,
  };

  return {
    hidden: { ...BASE_HIDDEN },
    visible: visibleTarget,
    exit: { ...BASE_EXIT, transition: { duration: 0.14, ease: "easeIn" } },
  };
}

// Render the glyph text per kind (design review v3 #6). The dot is a glyph
// variant in itself, not a universal prefix.
export function glyphText(kind: GlyphKind): string {
  switch (kind) {
    case "thinking":
      return "···";
    case "sigh":
      return "· 呼…";
    case "surprise":
      return "嗯？";
    case "sleepy":
      return "呜…";
  }
}
