import type { GuessRow, TileState } from './types';
import { STATE_ORDER } from './types';

/**
 * Minimum expression length supported by the UI.
 * Mirrors the backend's `MIN_SOLVE_LENGTH`.
 */
export const MIN_LENGTH = 3;

/**
 * Maximum expression length the UI will render guess boxes for.
 *
 * The backend supports arbitrary expression lengths, but rendering one tile
 * per character means an extremely large value (e.g. 1,000,000) would
 * allocate millions of DOM nodes and freeze the browser (see GitHub
 * issue #29). 64 is well above any realistic Sumzle expression length
 * while keeping the rendered tile count small enough to stay responsive.
 */
export const MAX_LENGTH = 64;

/** Clamp a candidate expression length to the supported range. */
export function clampLength(value: number): number {
  if (!Number.isFinite(value)) return MIN_LENGTH;
  return Math.min(MAX_LENGTH, Math.max(MIN_LENGTH, Math.trunc(value)));
}

/** Create a blank guess row with given length */
export function createBlankRow(length: number): GuessRow {
  return {
    tiles: Array.from({ length }, () => ({
      char: '',
      state: 'empty' as TileState,
    })),
  };
}

/** Cycle tile state: correct → present → empty → correct */
export function cycleState(current: TileState): TileState {
  const idx = STATE_ORDER.indexOf(current);
  return STATE_ORDER[(idx + 1) % STATE_ORDER.length];
}

/** Format search speed for display */
export function formatSpeed(speed: number): string {
  if (speed >= 1_000_000) {
    return `${(speed / 1_000_000).toFixed(1)}M`;
  }
  if (speed >= 1_000) {
    return `${(speed / 1_000).toFixed(0)}K`;
  }
  return speed.toString();
}

/** Format elapsed time for display */
export function formatTime(ms: number): string {
  if (ms >= 60_000) {
    const mins = Math.floor(ms / 60_000);
    const secs = ((ms % 60_000) / 1000).toFixed(1);
    return `${mins}m ${secs}s`;
  }
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(2)}s`;
  }
  return `${ms}ms`;
}
