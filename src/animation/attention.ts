// Attention States: NPC to living being.
// Design doc 6.6: Three attention tiers based on mouse proximity.
// Focused: mouse on the pet -> eye contact, slightly shy/playful
// Peripheral: mouse nearby -> head turns toward cursor
// Ignored: mouse far away -> resumes own life, occasionally glances

export enum AttentionState {
    Focused = "focused",
    Peripheral = "peripheral",
    Ignored = "ignored",
}

export interface PetRect {
    centerX: number;
    centerY: number;
    width: number;
    height: number;
}

const PERIPHERAL_RADIUS = 200;

export function computeAttention(
    mouseX: number,
    mouseY: number,
    pet: PetRect,
): AttentionState {
    const halfW = pet.width / 2;
    const halfH = pet.height / 2;
    if (
        mouseX >= pet.centerX - halfW &&
        mouseX <= pet.centerX + halfW &&
        mouseY >= pet.centerY - halfH &&
        mouseY <= pet.centerY + halfH
    ) {
        return AttentionState.Focused;
    }

    const dx = mouseX - pet.centerX;
    const dy = mouseY - pet.centerY;
    const dist = Math.sqrt(dx * dx + dy * dy);
    if (dist < PERIPHERAL_RADIUS) {
        return AttentionState.Peripheral;
    }

    return AttentionState.Ignored;
}

// Returns head angle (-1 to 1) for X and Y based on mouse position.
// Focused: centered (0, 0). Peripheral: angled toward cursor, clamped.
export function computeHeadAngle(
    mouseX: number,
    mouseY: number,
    pet: PetRect,
): { angleX: number; angleY: number } {
    const dx = mouseX - pet.centerX;
    const dy = mouseY - pet.centerY;
    if (dx === 0 && dy === 0) return { angleX: 0, angleY: 0 };
    const normX = Math.max(-1, Math.min(1, dx / PERIPHERAL_RADIUS));
    const normY = Math.max(-1, Math.min(1, dy / PERIPHERAL_RADIUS));
    return { angleX: normX, angleY: normY };
}
