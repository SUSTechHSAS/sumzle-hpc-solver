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
