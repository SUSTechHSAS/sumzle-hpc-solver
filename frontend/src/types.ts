/** Tile feedback state from the game */
export type TileState = 'correct' | 'present' | 'empty';

/** State cycle order */
export const STATE_ORDER: TileState[] = ['correct', 'present', 'empty'];

/** A single tile in a guess row */
export interface Tile {
  char: string;
  state: TileState;
}

/** A guess row consisting of tiles */
export interface GuessRow {
  tiles: Tile[];
}

/** Request body for POST /api/solve */
export interface SolveRequest {
  length: number;
  rows: GuessRow[];
}

/** Solver statistics */
export interface SolverStats {
  searched_count: number;
  found_count: number;
  elapsed_ms: number;
  speed: number;
}

/** Response from POST /api/solve */
export interface SolveResponse {
  solutions: string[];
  stats: SolverStats;
}

/** Request body for POST /api/validate */
export interface ValidateRequest {
  equation: string;
}

/** Response from POST /api/validate */
export interface ValidateResponse {
  valid: boolean;
}

/** Request body for POST /api/eval */
export interface EvalRequest {
  expression: string;
}

/** Response from POST /api/eval */
export interface EvalResponse {
  result: string | null;
}
