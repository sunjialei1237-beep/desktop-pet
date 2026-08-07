// Sleep-related pure predicates, extracted from App.tsx so the DeepNight
// auto-sleep trigger (design doc / Tier3 A4) is unit-testable without the FSM
// tick timer or the React runtime.
import { TimeOfDay } from "./circadian";
import { BehaviorState } from "./fsm";

export interface AutoSleepInput {
    /** Current circadian period (only DeepNight allows auto-sleep). */
    period: TimeOfDay;
    /** Current FSM state. */
    state: BehaviorState;
    /** True while waiting on an LLM reply ( Thinking). */
    isThinking: boolean;
    /** True while a reply bubble is streaming ( Talking). */
    isTalking: boolean;
    /** Milliseconds since the last user interaction. */
    idleMs: number;
    /** Idle threshold (ms) after which she drifts off. */
    thresholdMs: number;
}

/**
 * DeepNight (2-6) drift-off condition. She falls asleep only when all hold:
 *  - it is DeepNight (circadian),
 *  - she isn't already sleeping,
 *  - no conversation is happening (not thinking / not talking),
 *  - the user has left her alone strictly longer than `thresholdMs`.
 * markInteraction() refreshes the idle clock on any poke/pet/drag/chat/double
 * -click, so once she wakes the threshold must elapse again before re-sleep.
 * Mirrors the inline condition that lived in App.tsx's FSM-tick effect.
 */
export function shouldAutoSleep(opts: AutoSleepInput): boolean {
    return (
        opts.period === TimeOfDay.DeepNight &&
        opts.state !== BehaviorState.Sleeping &&
        !opts.isThinking &&
        !opts.isTalking &&
        opts.idleMs > opts.thresholdMs
    );
}
