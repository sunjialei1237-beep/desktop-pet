// Bubble — the pet's single speech surface (shadcn-style open code, not a
// black-box lib). Structure + a11y cues borrowed from shadcn's official Bubble
// (a "presentation surface"; conversation semantics live outside, so no
// aria-live here — see a11y note below). Animation is Motion.
//
// One organism, many moods: this component renders the SAME base animation for
// every emotion; only the parameters change (see bubbleVariants.ts). It is
// never a "different UI per emotion".
//
// Identity: the parent drives <Bubble key={bubbleId}>. A new bubbleId = a new
// speech act = exit the old + enter the new (AnimatePresence). Streaming token
// growth keeps the same bubbleId, so it does NOT replay the enter animation.
//
// a11y future: do NOT put aria-live on this node — streaming text updates every
// 22-72ms would make a screen reader announce constantly. When we add a11y,
// add a visually-hidden aria-live="polite" announcer at the App level that
// receives the FULL text only once, at stream end.

import { motion, type Variants } from "motion/react";
import { createBubbleVariants, glyphText, type BubbleEmotion, type GlyphKind } from "../animation/bubbleVariants";

export interface BubbleProps {
  text: string;
  emotion: BubbleEmotion;
  glyphKind?: GlyphKind; // only used when emotion === "bubble-glyph"
  pos?: string; // "" (default) | "bubble-pet" (petted-head position)
}

// Variants are derived per-render from the emotion — cheap, and avoids stale
// closures if emotion changes on the same bubble instance (defensive; normally
// a new emotion means a new bubbleId → new instance anyway).
export function Bubble({ text, emotion, glyphKind, pos = "" }: BubbleProps) {
  const variants: Variants = createBubbleVariants(emotion);
  const isGlyph = emotion === "bubble-glyph";

  // Glyph branch: a shell-less wordless signal (pure text + dot, no bubble
  // chrome). Distinct visual language from speech — it's a non-verbal cue.
  if (isGlyph) {
    return (
      <motion.span
        className="chat-glyph"
        variants={variants}
        initial="hidden"
        animate="visible"
        exit="exit"
      >
        {glyphKind ? glyphText(glyphKind) : text}
      </motion.span>
    );
  }

  // Speech branch: the warm-white bubble with a tail pointing down-left at Liri.
  // no-tail class is NOT used here (glyph handles its own branch above), but the
  // className stays data-driven for the position variant.
  return (
    <motion.div
      className={`chat-bubble ${pos}`}
      variants={variants}
      initial="hidden"
      animate="visible"
      exit="exit"
    >
      {text}
    </motion.div>
  );
}
