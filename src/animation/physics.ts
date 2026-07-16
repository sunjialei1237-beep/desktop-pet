// Physics: gravity, drag-and-drop, bounce.
// Design doc 6.9: Drag to mid-air -> release -> free fall -> bounce on ground.
// Body layer runs independently of LLM (Principle 5: Mind-Body decoupling).

const GRAVITY = 800; // px/s^2
const BOUNCE_DAMPING = 0.3;
const STOP_THRESHOLD = 50; // px/s, below this we snap to ground
const TASKBAR_HEIGHT = 48;

export interface PetPosition {
  x: number;
  y: number;
}

export interface ScreenBounds {
  width: number;
  height: number;
}

export type PhysicsPhase = "grounded" | "falling" | "dragging";

export class Physics {
  velocityY = 0;
  phase: PhysicsPhase = "grounded";

  get isGrounded() {
    return this.phase === "grounded";
  }

  startDrag() {
    this.phase = "dragging";
    this.velocityY = 0;
  }

  release() {
    if (this.phase === "dragging") {
      this.phase = "falling";
      this.velocityY = 0;
    }
  }

  // Returns updated position and whether a bounce/land event occurred
  update(
    pos: PetPosition,
    dt: number,
    bounds: ScreenBounds,
  ): { pos: PetPosition; landed: boolean; bounced: boolean } {
    if (this.phase !== "falling") {
      return { pos, landed: false, bounced: false };
    }

    let newY = pos.y;
    let bounced = false;
    let landed = false;

    this.velocityY += GRAVITY * dt;
    newY += this.velocityY * dt;

    const groundY = bounds.height - TASKBAR_HEIGHT - 120; // pet height approx

    if (newY >= groundY) {
      newY = groundY;
      if (Math.abs(this.velocityY) < STOP_THRESHOLD) {
        this.velocityY = 0;
        this.phase = "grounded";
        landed = true;
      } else {
        this.velocityY = -this.velocityY * BOUNCE_DAMPING;
        bounced = true;
      }
    }

    return { pos: { x: pos.x, y: newY }, landed, bounced };
  }

  // Snap to a specific position (e.g. during drag)
  snapTo(x: number, y: number): PetPosition {
    return { x, y };
  }
}
