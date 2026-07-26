// Position types for the desktop pet.
// 方案 B: no in-window free-fall. Moving the pet = moving the OS window.
// petPos = window top-left in screen coordinates.

export interface PetPosition {
  x: number;
  y: number;
}

export interface ScreenBounds {
  width: number;
  height: number;
}
