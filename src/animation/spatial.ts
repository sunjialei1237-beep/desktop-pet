// Spatial Memory: the pet has a "nest" she returns to.
// Design doc 6.8: First appearance picks a random corner, stays there.
// After 30s away from nest (not interacting), she walks back.

import type { PetPosition } from "./physics";

const RETURN_DELAY = 900; // seconds before auto-return (15 min, per user 08-01)
const WALK_SPEED = 60; // px/s

export class SpatialMemory {
  nestPosition: PetPosition | null = null;
  returnTimer = 0; // seconds accumulated away from nest

  // 方案 B: the nest is the initial window position in screen coordinates.
  setNest(pos: PetPosition) {
    this.nestPosition = { ...pos };
  }

  // Called each tick. If away from nest and not interacting, count down.
  // Returns the target position to move toward, or null if at nest.
  tick(
    currentPos: PetPosition,
    dt: number,
    isInteracting: boolean,
    isGrounded: boolean,
  ): { newPos: PetPosition; isWalking: boolean } {
    if (!this.nestPosition) return { newPos: currentPos, isWalking: false };

    const dx = this.nestPosition.x - currentPos.x;
    const dy = this.nestPosition.y - currentPos.y;
    const dist = Math.sqrt(dx * dx + dy * dy);

    // At nest, reset timer
    if (dist < 15) {
      this.returnTimer = 0;
      return { newPos: currentPos, isWalking: false };
    }

    // Away from nest
    if (!isInteracting && isGrounded) {
      this.returnTimer += dt;
      if (this.returnTimer >= RETURN_DELAY) {
        // Walk toward nest
        const step = Math.min(WALK_SPEED * dt, dist);
        const ratio = step / dist;
        return {
          newPos: {
            x: currentPos.x + dx * ratio,
            y: currentPos.y + dy * ratio,
          },
          isWalking: true,
        };
      }
    }

    return { newPos: currentPos, isWalking: false };
  }

  // Check if we're at nest
  isAtNest(pos: PetPosition): boolean {
    if (!this.nestPosition) return true;
    const dx = this.nestPosition.x - pos.x;
    const dy = this.nestPosition.y - pos.y;
    return Math.sqrt(dx * dx + dy * dy) < 15;
  }
}
