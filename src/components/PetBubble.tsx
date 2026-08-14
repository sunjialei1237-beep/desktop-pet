// PetBubble — the pet's speech surface, designed for a desktop companion
// (NOT a chat application). Adapted from a design that deliberately rejects
// "UI component" tropes: no border, no pill shape, irregular corner radii, an
// organically-shaped clip-path tail, and a shell-less glyph mode for wordless
// signals. Animation is Motion.
//
// Variant mapping: the rest of the app passes BubbleEmotion ("bubble-calm" etc.
// from bubbleClassForMood / showBubble calls). This component accepts BOTH the
// "bubble-xxx" form and the bare "xxx" form, so call sites need no changes.

import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, type TargetAndTransition, type Transition } from "motion/react";

export type PetBubbleVariant =
  | "calm"
  | "happy"
  | "playful"
  | "sad"
  | "worried"
  | "shy"
  | "glyph";

export type PetBubbleMode = "speech" | "glyph";

export interface PetBubbleProps {
  visible: boolean;
  text: string;
  bubbleId: string | number;
  /** Accepts both "bubble-calm" (app's BubbleEmotion) and bare "calm". */
  variant?: PetBubbleVariant | string;
  mode?: PetBubbleMode;
  /** Tail direction. MVP default left-bottom (toward Liri). */
  tail?: "left-bottom" | "right-bottom";
  /** Max width in px. Desktop pet should stay small — 180~210 recommended. */
  maxWidth?: number;
  className?: string;
  /** Reports the bubble's viewport rect (CSS px) so App keeps the window
      non-transparent over it — under OS-level click-through (setIgnoreCursorEvents)
      CSS pointer-events alone can't make the bubble scrollable; the window must
      stop ignoring the cursor over the bubble rect. Null when hidden. */
  onBubbleBounds?: (rect: { left: number; top: number; width: number; height: number } | null) => void;
}

const ENTER_TRANSITION: Transition = {
  duration: 0.28,
  ease: [0.22, 1, 0.36, 1],
};

const EXIT_TRANSITION: Transition = {
  duration: 0.16,
  ease: [0.4, 0, 1, 1],
};

interface MotionConfig {
  initial?: TargetAndTransition;
  animate?: TargetAndTransition;
  transition?: Transition;
}

const MOTION_BY_VARIANT: Record<Exclude<PetBubbleVariant, "glyph">, MotionConfig> = {
  calm: {
    initial: { opacity: 0, y: 5, scale: 0.985 },
    animate: { opacity: 1, y: 0, scale: 1 },
    transition: ENTER_TRANSITION,
  },
  happy: {
    initial: { opacity: 0, y: 5, scale: 0.98 },
    animate: { opacity: 1, y: 0, scale: 1 },
    transition: { type: "spring", stiffness: 200, damping: 22, mass: 0.7 },
  },
  playful: {
    initial: { opacity: 0, y: 5, scale: 0.98 },
    animate: { opacity: 1, y: 0, scale: 1, rotate: [0, -0.5, 0.5, 0] },
    transition: { type: "spring", stiffness: 210, damping: 20, mass: 0.7 },
  },
  sad: {
    initial: { opacity: 0, y: 7, scale: 0.988 },
    animate: { opacity: 1, y: 1, scale: 1 },
    transition: { duration: 0.42, ease: [0.22, 1, 0.36, 1] },
  },
  worried: {
    initial: { opacity: 0, y: 5, scale: 0.987 },
    animate: { opacity: 1, y: 0, scale: 1, rotate: [0, -0.25, 0.25, 0] },
    transition: { duration: 0.4, ease: "easeOut" },
  },
  shy: {
    initial: { opacity: 0, y: 4, scale: 0.99 },
    animate: { opacity: [0, 0.45, 1], y: 0, scale: 1 },
    transition: { duration: 0.9, ease: [0.22, 1, 0.36, 1] },
  },
};

function getMotionConfig(variant: PetBubbleVariant): MotionConfig {
  if (variant === "glyph") {
    return {
      initial: { opacity: 0, y: 5, scale: 0.985 },
      animate: { opacity: 1, y: 0, scale: 1 },
      transition: { duration: 0.3, ease: [0.22, 1, 0.36, 1] },
    };
  }
  return MOTION_BY_VARIANT[variant];
}

// Normalize the app's "bubble-xxx" style strings into the bare variant name.
function normalizeVariant(v: string): PetBubbleVariant {
  if (v.startsWith("bubble-")) return v.slice(7) as PetBubbleVariant;
  return v as PetBubbleVariant;
}

export function PetBubble({
  visible,
  text,
  bubbleId,
  variant = "calm",
  mode,
  tail = "left-bottom",
  maxWidth = 200,
  className = "",
  onBubbleBounds,
}: PetBubbleProps) {
  const v = normalizeVariant(String(variant));
  const isGlyph = mode === "glyph" || v === "glyph";
  const motionConfig = getMotionConfig(v);
  // True once text actually exceeds the height cap. Below the cap the bubble
  // stays overflow-y:hidden (no scrollbar, not user-scrollable — it just
  // streams/grows with the text); .pet-bubble--scrollable flips it to
  // overflow-y:auto so the scrollbar appears and scrolling is allowed. JS
  // drives this instead of relying on native overflow-y:auto alone, because
  // on some WebView2/Windows setups auto renders a persistent scrollbar
  // track even when content fits (verified: "你好呀" showed a track).
  const [overflowing, setOverflowing] = useState(false);
  // Auto-scroll to bottom on text change: streaming appends tokens at the end,
  // and a capped-height bubble would otherwise keep the newest text clipped
  // below the fold. The user can still scroll up freely to re-read earlier
  // parts (bidirectional scroll — "从上往下翻" reads top→down, scroll-up
  // revisits the start).
  const bubbleRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = bubbleRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [text]);
  // Detect overflow: scrollHeight > clientHeight means the content is taller
  // than the max-height cap (works under overflow-y:hidden too — scrollHeight
  // includes clipped content). Re-measured on every text change (streaming),
  // on show/hide, and on bubble identity change so a fresh bubble never
  // inherits the previous bubble's overflow state. +1 absorbs sub-pixel
  // rounding right at the boundary.
  useEffect(() => {
    const el = bubbleRef.current;
    if (!el) return;
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
  }, [text, visible, bubbleId]);
  // Report the bubble rect so App can treat it as a non-click-through region
  // (required for scrolling under OS-level ignore_cursor_events). Null when
  // hidden (or glyph mode, which has no scrollable .pet-bubble) so App stops
  // keeping the window opaque over it. Re-measures on text change (streaming
  // appends grow the bubble).
  useEffect(() => {
    if (!onBubbleBounds) return;
    if (visible && bubbleRef.current) {
      const r = bubbleRef.current.getBoundingClientRect();
      onBubbleBounds({ left: r.left, top: r.top, width: r.width, height: r.height });
    } else {
      onBubbleBounds(null);
    }
  }, [visible, text, onBubbleBounds]);

  return (
    <AnimatePresence initial={false} mode="sync">
      {visible && (
        // Single layer (motion.div owns both positioning + animation). Inline
        // style has the highest specificity so `left` is honored. NO
        // translateX(-50%) centering — that made a wide streaming bubble shift
        // left (−50% of a wide box is a big shift). Fixed left edge keeps every
        // bubble at the same spot regardless of width. We verified inline left
        // works on this single-layer structure (the 320px diag test moved it
        // right); the two-layer split broke it.
        <motion.div
          key={bubbleId}
          className={[
            "pet-bubble-anchor",
            isGlyph ? "pet-bubble-anchor--glyph" : "",
            className,
          ]
            .filter(Boolean)
            .join(" ")}
          initial={motionConfig.initial}
          animate={motionConfig.animate}
          exit={{ opacity: 0, y: 6, scale: 0.985, transition: EXIT_TRANSITION }}
          transition={motionConfig.transition}
          style={
            {
              "--pet-bubble-max-width": `${maxWidth}px`,
              "--pet-bubble-tail-direction": tail === "right-bottom" ? "-1" : "1",
              // Tail-tip anchored at Liri's head top-right, FIXED for every
              // bubble (speech/glyph, all variants, all call sites — no
              // position variant overrides). Window is 400x760; the model is
              // centered in the 400x600 canvas whose top sits at window y=150,
              // and the head (back-hair mass + right ear) occupies roughly
              // window x[170,230], y[240,330]. The tail tip sits at the bubble
              // body's bottom-left corner (tail is at left:15px of the body,
              // its tip at ~45% of its 17px width, and its bottom edge 10px
              // below the body — top tucked 2px INTO the body so the -5deg
              // rotation doesn't leave a visible gap at the seam) =>
              // tip ≈ (anchorLeft + 22, windowBottom - 10).
              // To land the tip at window (210, 235): left = 210-22 = 188,
              // bottom = 760-235+10 = 535. (User confirmed X, asked +20px up
              // from the original (210,255); then moved the tail down 5px to
              // clear the bubble body, then up 2px to remove the seam gap —
              // anchor compensated each time so the tip stays put.) Long
              // text grows upward/rightward, the tail stays put — that's the
              // "tail as anchor" contract.
              position: "absolute",
              bottom: "535px",
              left: "188px",
              zIndex: 50,
              pointerEvents: "none",
            } as React.CSSProperties
          }
          aria-hidden="true"
        >
          {isGlyph ? (
            <span className="pet-bubble-glyph">
              <span className="pet-bubble-glyph-mark">·</span>
              <span className="pet-bubble-glyph-text">{text}</span>
            </span>
          ) : (
            <>
              {/* .pet-bubble is the scroll container (overflow-y hidden/auto
                  via the overflowing class); the tail must NOT live inside it
                  — a scroll container clips to its padding box, which cut the
                  tail's lower half (bottom:-7px hangs outside). The tail is a
                  sibling so it stays fully visible; the anchor's box exactly
                  coincides with the bubble's (bubble is its only in-flow
                  child), so left:15px/bottom:-7px land in the same spot. */}
              <div
                ref={bubbleRef}
                className={`pet-bubble${overflowing ? " pet-bubble--scrollable" : ""}`}
              >
                <span className="pet-bubble-text">{text}</span>
              </div>
              <span className="pet-bubble-tail" />
            </>
          )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

export default PetBubble;
