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
  /** The top-N value used (0 means all solutions were returned). */
  top: number;
  /** Score of each solution, aligned with `solutions` by index. Empty unless
   * `top` > 0, in which case `solutions` is sorted by it (descending). */
  scores: number[];
}

/** Live progress emitted by POST /api/solve/progress (issue #22). */
export interface SolveProgress {
  /** Completed search branches. */
  done: number;
  /** Total search branches to complete (0 until the branch set is known). */
  total: number;
  /** Coarse phase: 0 preparing, 1 searching, 2 scoring (top-N), 3 finished. */
  phase: number;
}

/** Human-readable label for a {@link SolveProgress} phase. */
export function progressPhaseLabel(phase: number): string {
  switch (phase) {
    case 1:
      return '搜索中';
    case 2:
      return '评分中';
    case 3:
      return '完成';
    default:
      return '准备中';
  }
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
