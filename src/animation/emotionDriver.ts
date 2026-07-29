// Emotion -> Live2D continuous parameter baseline.
//
// Architecture Principle #10 (prioritize liveliness): the pet's face should
// reflect her internal emotion continuously, not jump between 6 discrete
// expression files. This pure function maps the emotion vector (mood/energy/
// stress/...) to Cubism4 parameter values written every frame as a BASELINE
// layer, layered UNDER the behavior overlay (behaviorDriver) which overrides
// per-frame for active microbehaviors (LookAround/Sleeping/etc.).
//
// Design doc (P10 emotionBridge): "continuous vector for Live2D parameter
// interpolation" -- eye_open / mouth_form / motion parameters.
//
// Param availability: all IDs below are verified present on Haru via its
// expression files (expressions/F01.exp3.json references ParamEyeLOpen,
// ParamMouthForm, ParamEyeForm). The caller still wraps writes in try/catch.
import type { BehaviorParams } from "./behaviorDriver";

/// The continuous emotion vector (0..1 each). Mirrors the backend
/// `EmotionResponse` / "emotion-update" event payload (loop_runner.rs).
export interface EmotionVector {
    mood: number;            // 0..1 (low = sad, high = happy)
    physical_energy: number; // 0..1 (low = tired)
    social_battery: number;  // 0..1 (low = drained)
    stress: number;          // 0..1 (high = anxious)
    loneliness: number;      // 0..1
    rest_need: number;       // 0..1 (high = sleepy)
}

/// Default vector — matches Rust `EmotionState::default()` so a freshly
/// launched pet (before the first DB read) looks calm/neutral, not broken.
export const DEFAULT_EMOTION: EmotionVector = {
    mood: 0.5,
    physical_energy: 0.7,
    social_battery: 0.8,
    stress: 0.2,
    loneliness: 0.0,
    rest_need: 0.0,
};

// Cubism4 parameter IDs (standard names; Haru uses these).
const P_EYE_L = "ParamEyeLOpen";
const P_EYE_R = "ParamEyeROpen";
const P_MOUTH_FORM = "ParamMouthForm";
const P_EYE_FORM = "ParamEyeForm";
const P_BROW_L = "ParamBrowLY";
const P_BROW_R = "ParamBrowRY";

// --- Tunables (adjust here) -------------------------------------------------
// Eyes: how strongly fatigue (low energy / high rest_need) droops the lids.
const EYE_OPEN_FULL = 1.0;
const EYE_OPEN_DROWSY = 0.3;    // floor: never fully shut from fatigue alone
const EYE_FATIGUE_AT_ENERGY = 0.6; // energy below this starts drooping the lids
const EYE_FATIGUE_GAIN = 1.4;
const EYE_REST_GAIN = 0.4;
// Mouth form: mood pulls toward a smile, stress toward a frown.
const MOUTH_GLUM = -0.25;
const MOUTH_SMILE = 0.7;
const MOUTH_STRESS_PULL = 0.45;
const MOUTH_FLOOR = -0.5;
const MOUTH_CEIL = 0.75;
// Eye form (smile-eye ^_^): only appears when clearly happy.
const EYE_FORM_HAPPY_AT = 0.55;
const EYE_FORM_GAIN = 1.8;
const EYE_FORM_CEIL = 0.8;
// Brow: low mood droops the brows (Haru F01.exp3 sad preset uses -0.56).
const BROW_GLUM = -0.4;

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

/// Map the emotion vector to a Live2D parameter baseline. Pure: same input ->
/// same output, frame after frame. The caller merges this UNDER the behavior
/// overlay (`{ ...emoParams, ...behParams }`) so active microbehaviors win.
///
/// Dimensions expressed (Principle #11 -- explainability):
///  - eye open   : fatigue (low energy + high rest_need) droops the lids.
///  - mouth form : mood -> smile; stress -> frown.
///  - eye form   : clear happiness -> crinkling smile-eyes (^_^).
///  - brow       : low mood droops the brows (direction verified vs Haru F01.exp3).
export function getEmotionParams(e: EmotionVector): BehaviorParams {
    // Eyes: fatigue (low energy / high rest_need) droops the lids.
    const fatigue =
        Math.max(0, EYE_FATIGUE_AT_ENERGY - e.physical_energy) * EYE_FATIGUE_GAIN +
        e.rest_need * EYE_REST_GAIN;
    const eye = clamp(EYE_OPEN_FULL - fatigue, EYE_OPEN_DROWSY, EYE_OPEN_FULL);

    // Mouth form: mood pulls toward a smile, stress toward a frown.
    const form = clamp(
        lerp(MOUTH_GLUM, MOUTH_SMILE, e.mood) - e.stress * MOUTH_STRESS_PULL,
        MOUTH_FLOOR,
        MOUTH_CEIL,
    );

    // Eye form: only when clearly happy do the eyes crinkle into a smile.
    const eyeForm = clamp(
        (e.mood - EYE_FORM_HAPPY_AT) * EYE_FORM_GAIN,
        0,
        EYE_FORM_CEIL,
    );

    // Brow: low mood droops the brows. Direction verified against Haru's sad
    // preset (F01.exp3 sets ParamBrowLY/RY = -0.56). Written every frame, this
    // also overrides brow params left residual by any preset expression switch.
    const brow = clamp(lerp(BROW_GLUM, 0, e.mood), BROW_GLUM, 0);

    return {
        [P_EYE_L]: eye,
        [P_EYE_R]: eye,
        [P_MOUTH_FORM]: form,
        [P_EYE_FORM]: eyeForm,
        [P_BROW_L]: brow,
        [P_BROW_R]: brow,
    };
}

/// Boost the emotion vector for a transient per-turn expression. The backend
/// (`react::transient_expression`) emits a Haru expression id -- "f00" (happy)
/// or "f04" (worried) -- when a strong signal is detected in the user's text.
/// We translate it to an emotion nudge so the SAME continuous-parameter path
/// expresses it, instead of switching to a preset expression file (which would
/// leave residual brow/face params when the transient ends). ~8s later the App
/// clears the transient and the vector returns to its accumulated baseline.
export function boostForTransientExpression(
    expr: string,
    base: EmotionVector,
): EmotionVector {
    if (expr === "f00") {
        // happy: lift mood clearly into the smile zone.
        return { ...base, mood: Math.max(base.mood, 0.88) };
    }
    if (expr === "f04") {
        // worried: raise stress, dip mood.
        return {
            ...base,
            stress: Math.max(base.stress, 0.7),
            mood: Math.min(base.mood, 0.35),
        };
    }
    return base;
}
