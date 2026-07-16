// Spatial Memory: the pet has a "nest" she returns to.
// Design doc 6.8: First appearance picks a random corner, stays there.
// After 30s away from nest (not interacting), she walks back.

import type { PetPosition, ScreenBounds } from "./physics";

const RETURN_DELAY = 30; // seconds before auto-return
const WALK_SPEED = 60; // px/s

export class SpatialMemory {
  nestPosition: PetPosition | null = null;
  returnTimer = 0; // seconds accumulated away from nest

  init(bounds: ScreenBounds): PetPosition {
    const corners: PetPosition[] = [
      { x: 50, y: bounds.height - 48 - 120 }, // bottom-left
      { x: bounds.width - 250, y: bounds.height - 48 - 120 }, // bottom-right
      { x: 50, y: 50 }, // top-left
      { x: bounds.width - 250, y: 50 }, // top-right
    ];
    this.nestPosition = corners[Math.floor(Math.random() * corners.length)];
    return { ...this.nestPosition };
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
