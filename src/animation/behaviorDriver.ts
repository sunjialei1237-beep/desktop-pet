// Behavior parameter driver: maps each FSM BehaviorState to a set of Cubism4
// parameter values to write every frame, layered ON TOP of the library's idle
// breathing/blink/motion. This is what makes LookAround really turn the head,
// TiltHead really tilt, Sway really sway, etc. -- without changing the model.
//
// Design: a PURE function of (behavior, elapsedMs). No state, no RNG (the FSM
// already randomizes which behavior fires and how long it lasts). The ticker in
// Live2DCanvas.tsx calls this each frame and writes the returned params with
// setParameterValueById; the library overwrites them next frame unless we keep
// writing them, so "last writer wins" gives us clean overlay control that ends
// automatically when the behavior ends (we just stop writing -> idle resumes).
//
// Param availability for Haru (verified against physics3.json + model3.json
// Groups): ParamAngleX, ParamAngleZ, ParamBodyAngleX, ParamBodyAngleZ,
// ParamHairFront/Side/Back, ParamEyeLOpen, ParamEyeROpen, ParamMouthOpenY all
// exist. ParamAngleY / ParamBreath / ParamMouthForm are NOT referenced by
// physics3.json -- they MAY not exist in the moc, so the caller wraps every
// setParameterValueById in try/catch and the behavior silently degrades to
// idle for any missing param.
import { BehaviorState } from "./fsm";

// --- Tunable constants (adjust here) ---------------------------------------
// Head turn (left/right). LookAround sweeps this with a sine.
const LOOKAROUND_AMP = 25; // degrees of head rotation
const LOOKAROUND_PERIOD_MS = 2500; // matches FSM look_around duration -> one sweep
// Head tilt. TiltHead holds this; a short ease-in makes it look natural.
const TILT_HEAD_HOLD = 12;
const TILT_EASE_MS = 220;
// Body sway. Sway oscillates the body.
const SWAY_AMP = 8;
const SWAY_PERIOD_MS = 3000; // matches FSM sway duration -> one sway cycle
// Yawn: slow mouth open, squint.
const YAWN_MOUTH_PERIOD_MS = 1500;
// Stretch: push body forward then release (half sine over the duration).
const STRETCH_BODY_AMP = 10;
const STRETCH_PERIOD_MS = 2000;
// Peek: quick head offset then settle back.
const PEEK_OFFSET = 18;
const PEEK_DECAY_MS = 700;
// Hum: amplified breathing.
const HUM_BREATH_AMP = 0.9;
const HUM_BREATH_PERIOD_MS = 1400;
// Sleeping: slow shallow breathing.
const SLEEP_BREATH_PERIOD_MS = 4000;
// Blink: quick close-open envelope (ms). FSM blink lasts 800ms but the actual
// lid dip only occupies this window, after which eyes release to the library.
const BLINK_DUR_MS = 220;

// Cubism4 parameter IDs (standard names; Haru uses these).
const P_ANGLE_X = "ParamAngleX";
const P_ANGLE_Y = "ParamAngleY"; // may not exist -> try/catch
const P_ANGLE_Z = "ParamAngleZ";
const P_BODY_X = "ParamBodyAngleX";
const P_EYE_L = "ParamEyeLOpen";
const P_EYE_R = "ParamEyeROpen";
const P_MOUTH_Y = "ParamMouthOpenY";
const P_BREATH = "ParamBreath"; // may not exist -> try/catch
const P_MOUTH_FORM = "ParamMouthForm"; // may not exist -> try/catch

/// The per-frame parameter overlay for a behavior. Empty = do not intervene
/// (library idle / focus / lipsync fully own the model this frame).
export type BehaviorParams = Record<string, number>;

/// One full sine wave that starts and ends at 0. For LookAround/Sway this gives
/// a single smooth sweep across the behavior's whole duration.
function sineWave(elapsedMs: number, periodMs: number, amp: number): number {
    return amp * Math.sin((elapsedMs / periodMs) * Math.PI * 2);
}

/// Ease from 0 -> 1 over `durMs`, clamped at 1 afterwards (for settle-in).
function easeIn(elapsedMs: number, durMs: number): number {
    return Math.min(1, elapsedMs / durMs);
}

/// Exponential decay from `start` toward 0 (for Peek's quick offset fading out).
function decay(elapsedMs: number, tauMs: number, start: number): number {
    return start * Math.exp(-elapsedMs / tauMs);
}

/// Eyes both open to the same value.
function eyes(open: number): BehaviorParams {
    return { [P_EYE_L]: open, [P_EYE_R]: open };
}

/// Compute the parameter overlay for the current behavior and how long it has
/// been active. Pure: same inputs -> same output, frame after frame.
///
/// Idle/Recovering/Talking return {} so the library owns those states entirely
/// (Talking must NOT touch the mouth -- lipsync owns ParamMouthOpenY).
export function getBehaviorParams(behavior: BehaviorState, elapsedMs: number): BehaviorParams {
    switch (behavior) {
        case BehaviorState.LookAround: {
            // Head sweeps left-right; body and mouth pinned still, eyes left to library.
            return {
                [P_ANGLE_X]: sineWave(elapsedMs, LOOKAROUND_PERIOD_MS, LOOKAROUND_AMP),
                [P_ANGLE_Z]: 0,
                [P_BODY_X]: 0,
                [P_MOUTH_Y]: 0,
            };
        }
        case BehaviorState.TiltHead: {
            // Settle into a held tilt, body still.
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: TILT_HEAD_HOLD * easeIn(elapsedMs, TILT_EASE_MS),
                [P_BODY_X]: 0,
                [P_MOUTH_Y]: 0,
            };
        }
        case BehaviorState.Sway: {
            // Body rocks side to side, head neutral, eyes left to library.
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: 0,
                [P_BODY_X]: sineWave(elapsedMs, SWAY_PERIOD_MS, SWAY_AMP),
                [P_MOUTH_Y]: 0,
            };
        }
        case BehaviorState.Yawn: {
            // Slow mouth open (deep breath in), squint, slight tilt.
            const mouth = 0.5 - 0.5 * Math.cos((elapsedMs / YAWN_MOUTH_PERIOD_MS) * Math.PI);
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: 5,
                [P_BODY_X]: 0,
                ...eyes(0.3),
                [P_MOUTH_Y]: Math.min(0.8, mouth),
            };
        }
        case BehaviorState.Stretch: {
            // Push body forward then ease back (half sine arc), relaxed eyes.
            const body = STRETCH_BODY_AMP * Math.sin((elapsedMs / STRETCH_PERIOD_MS) * Math.PI);
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: 0,
                [P_BODY_X]: Math.max(0, body),
                ...eyes(0.5),
                [P_MOUTH_Y]: 0.3,
            };
        }
        case BehaviorState.Peek: {
            // Quick head offset that fades, half-open eyes (peeking), body still.
            return {
                [P_ANGLE_X]: decay(elapsedMs, PEEK_DECAY_MS, PEEK_OFFSET),
                [P_ANGLE_Z]: 6,
                [P_BODY_X]: 0,
                ...eyes(0.4),
                [P_MOUTH_Y]: 0,
            };
        }
        case BehaviorState.Hum: {
            // Amplified breathing; mouth shape shifts (smile/hum). Head/body pinned.
            const breath = 0.5 + 0.5 * Math.sin((elapsedMs / HUM_BREATH_PERIOD_MS) * Math.PI * 2);
            const form = 0.6 * Math.sin((elapsedMs / HUM_BREATH_PERIOD_MS) * Math.PI * 2);
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: 0,
                [P_BODY_X]: 0,
                [P_BREATH]: HUM_BREATH_AMP * Math.max(0.3, breath),
                [P_MOUTH_FORM]: form,
            };
        }
        case BehaviorState.Blink: {
            // Natural lid dip only within the envelope; after that release to library.
            if (elapsedMs > BLINK_DUR_MS) return {};
            // 1 -> 0 -> 1: open at edges, closed at the midpoint.
            const open = 1 - Math.abs(Math.sin((elapsedMs / BLINK_DUR_MS) * Math.PI));
            return { ...eyes(open), [P_MOUTH_Y]: 0 };
        }
        case BehaviorState.Thinking: {
            // Slight tilt down (ParamAngleY if present), tilted head, half eyes.
            return {
                [P_ANGLE_Y]: 10,
                [P_ANGLE_Z]: 8,
                [P_BODY_X]: 0,
                ...eyes(0.6),
                [P_MOUTH_Y]: 0,
            };
        }
        case BehaviorState.Sleeping: {
            // Eyes shut, slow shallow breathing, everything else pinned.
            const breath = 0.2 + 0.3 * (0.5 + 0.5 * Math.sin((elapsedMs / SLEEP_BREATH_PERIOD_MS) * Math.PI * 2));
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: 0,
                [P_BODY_X]: 0,
                ...eyes(0),
                [P_MOUTH_Y]: 0,
                [P_BREATH]: breath,
            };
        }
        case BehaviorState.Embarrassed: {
            // Tilt away, half eyes, small mouth.
            return {
                [P_ANGLE_X]: 0,
                [P_ANGLE_Z]: -10,
                [P_BODY_X]: 0,
                ...eyes(0.4),
                [P_MOUTH_Y]: 0.2,
            };
        }
        case BehaviorState.Idle:
        case BehaviorState.Recovering:
        case BehaviorState.Talking:
        default:
            // Do not intervene: library idle breathing / focus / lipsync own it.
            return {};
    }
}

/// Params that may not exist on the model (Haru's physics3.json does not
/// reference these). The caller's try/catch already degrades gracefully; this
/// list is exported for diagnostics/reporting.
export const POSSIBLY_MISSING_PARAMS = [P_ANGLE_Y, P_BREATH, P_MOUTH_FORM];
