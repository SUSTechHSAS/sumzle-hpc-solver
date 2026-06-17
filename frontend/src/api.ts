import type {
  SolveRequest,
  SolveResponse,
  ValidateRequest,
  ValidateResponse,
  EvalRequest,
  EvalResponse,
  DownloadFormat,
} from './types';

// Detect Tauri at runtime. Tauri injects `window.__TAURI_INTERNALS__` (and the
// `@tauri-apps/api` package keys off it). In dev/standalone web mode this is
// absent and we fall through to the regular HTTP API.
const isTauri =
  typeof window !== 'undefined' &&
  // @ts-ignore - injected by Tauri runtime
  (window.__TAURI_INTERNALS__ !== undefined || window.__TAURI__ !== undefined);

// Lazily import the Tauri invoke bridge only when running inside Tauri, so the
// web build does not pull in the (optional) Tauri runtime.
let invokeFn: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!invokeFn) {
    const mod = await import('@tauri-apps/api/core');
    invokeFn = mod.invoke;
  }
  return invokeFn(cmd, args) as Promise<T>;
}

const API_BASE = '/api';

export interface SolveOptions {
  /** Number of threads (0 = auto, 1 = single-threaded). Default: 0. */
  threads?: number;
  /** Return only the top-N highest-scoring solutions (0 = return all). Default: 0. */
  top?: number;
}

export async function solvePuzzle(
  length: number,
  rows: SolveRequest['rows'],
  options: SolveOptions = {},
): Promise<SolveResponse> {
  const threads = options.threads ?? 0;
  const top = options.top ?? 0;

  if (isTauri) {
    // Tauri command: same JSON shape as the HTTP body, with options flattened
    // into top-level fields (matching the Rust `SolveRequest` struct).
    return invoke<SolveResponse>('solve', {
      req: { length, rows, threads, top },
    });
  }

  const params = new URLSearchParams({ threads: String(threads) });
  if (top > 0) {
    params.set('top', String(top));
  }
  const res = await fetch(`${API_BASE}/solve?${params}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ length, rows }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

/**
 * Generate and download results directly from frontend data.
 * This avoids re-running the solver on the backend, saving server CPU and memory.
 */
export function downloadResults(
  data: SolveResponse,
  format: DownloadFormat = 'json',
): void {
  const timestamp = Math.floor(Date.now() / 1000);
  // Guard against an older backend that omits `scores`: reading `.length` on
  // an undefined field would throw a TypeError.
  const hasScores =
    !!data.scores && data.scores.length === data.solutions.length && data.scores.length > 0;
  let content: string;
  let mimeType: string;
  let extension: string;

  switch (format) {
    case 'csv': {
      const lines = [hasScores ? 'index,expression,score' : 'index,expression'];
      data.solutions.forEach((sol, i) => {
        lines.push(hasScores ? `${i + 1},${sol},${data.scores[i]}` : `${i + 1},${sol}`);
      });
      content = lines.join('\n');
      mimeType = 'text/csv;charset=utf-8';
      extension = 'csv';
      break;
    }
    case 'txt': {
      const lines: string[] = [
        'Sumzle Solver Results',
        '=====================',
        `Solutions found: ${data.stats.found_count}`,
        `Expressions searched: ${data.stats.searched_count}`,
        `Time elapsed: ${data.stats.elapsed_ms}ms`,
        `Search speed: ${data.stats.speed} expr/s`,
      ];
      if (data.top > 0) {
        lines.push(`Top-N: ${data.top}`);
      }
      if (data.recommended) {
        lines.push(`Recommended: ${data.recommended}`);
      }
      lines.push('', '--- Solutions ---');
      data.solutions.forEach((sol, i) => {
        lines.push(
          hasScores ? `${i + 1}. ${sol} (score: ${data.scores[i].toFixed(2)})` : `${i + 1}. ${sol}`,
        );
      });
      content = lines.join('\n');
      mimeType = 'text/plain;charset=utf-8';
      extension = 'txt';
      break;
    }
    default: {
      // JSON format
      content = JSON.stringify(data, null, 2);
      mimeType = 'application/json;charset=utf-8';
      extension = 'json';
      break;
    }
  }

  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `sumzle_solutions_${timestamp}.${extension}`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export async function validateEquation(equation: string): Promise<ValidateResponse> {
  if (isTauri) {
    return invoke<ValidateResponse>('validate', { equation });
  }
  const body: ValidateRequest = { equation };
  const res = await fetch(`${API_BASE}/validate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function evaluateExpression(expression: string): Promise<EvalResponse> {
  if (isTauri) {
    return invoke<EvalResponse>('eval_expression', { expression });
  }
  const body: EvalRequest = { expression };
  const res = await fetch(`${API_BASE}/eval`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}
