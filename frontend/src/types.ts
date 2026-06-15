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

/** Character probability entry */
export interface CharProbability {
  char: string;
  display: string;
  count: number;
  probability: number;
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
  char_probabilities: CharProbability[];
  recommended: string | null;
  /** The top-N value used (0 means all solutions were returned) */
  top: number;
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

/** Download format options */
export type DownloadFormat = 'json' | 'csv' | 'txt';

/** Valid characters for the virtual keyboard */
export const VALID_CHARS = [
  ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
  ['+', '-', '*', '/', '%', '^', '=', '>', '!'],
  ['A', '(', ')', '[', ']', '⌫'],
];

/** Display character mappings for solver output */
export const CHAR_DISPLAY: Record<string, string> = {
  '*': '×',
  '/': '÷',
};

/** Convert a raw solver character to its display form */
export function displayChar(ch: string): string {
  return CHAR_DISPLAY[ch] || ch;
}
