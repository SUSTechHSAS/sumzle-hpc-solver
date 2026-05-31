import type { GuessRow, TileState } from './types';
import { STATE_ORDER } from './types';

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
