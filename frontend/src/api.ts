import type {
  SolveRequest,
  SolveResponse,
  SolveProgress,
  ValidateRequest,
  ValidateResponse,
  EvalRequest,
  EvalResponse,
  DownloadFormat,
} from './types';
import { invoke, isTauri, Channel } from '@tauri-apps/api/core';

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
  // Standalone app (Tauri): call the embedded Rust solver over IPC — no HTTP.
  if (isTauri()) {
    return invoke<SolveResponse>('solve', { length, rows, threads, top });
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
  if (isTauri()) {
    return { valid: await invoke<boolean>('validate', { equation }) };
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
  if (isTauri()) {
    return { result: await invoke<string | null>('eval', { expression }) };
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

/** Build the shared `?threads=&top=` query string for the solve endpoints. */
function solveParams(options: SolveOptions, includeTop: boolean): string {
  const params = new URLSearchParams({ threads: String(options.threads ?? 0) });
  const top = options.top ?? 0;
  if (includeTop && top > 0) {
    params.set('top', String(top));
  }
  return params.toString();
}

/** Parse one SSE frame into its `event` name and concatenated `data` payload.
 * Returns null for comment-only/empty frames (e.g. keep-alive `:` lines). */
function parseSseFrame(frame: string): { event: string; data: string } | null {
  let event = 'message';
  const dataLines: string[] = [];
  for (const raw of frame.split('\n')) {
    const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw;
    if (line.startsWith(':')) continue; // comment / keep-alive
    if (line.startsWith('event:')) {
      event = line.slice(6).trim();
    } else if (line.startsWith('data:')) {
      const rest = line.slice(5);
      dataLines.push(rest.startsWith(' ') ? rest.slice(1) : rest);
    }
  }
  if (dataLines.length === 0) return null;
  return { event, data: dataLines.join('\n') };
}

/**
 * Solve via the streaming SSE endpoint, reporting live progress (issue #22).
 *
 * Identical result to {@link solvePuzzle}, but the backend streams
 * `progress` events (forwarded to `onProgress`) before the final result, so the
 * UI can show a real, branch-completion-based progress bar.
 */
export async function solveWithProgress(
  length: number,
  rows: SolveRequest['rows'],
  options: SolveOptions = {},
  onProgress?: (progress: SolveProgress) => void,
): Promise<SolveResponse> {
  // Standalone app (Tauri): stream progress over an IPC Channel, then resolve
  // with the final result — same contract as the SSE path below.
  if (isTauri()) {
    const channel = new Channel<SolveProgress>();
    if (onProgress) channel.onmessage = onProgress;
    return invoke<SolveResponse>('solve_progress', {
      length,
      rows,
      threads: options.threads ?? 0,
      top: options.top ?? 0,
      onProgress: channel,
    });
  }
  const res = await fetch(`${API_BASE}/solve/progress?${solveParams(options, true)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ length, rows }),
  });
  if (!res.ok) throw new Error(await res.text());
  if (!res.body) throw new Error('当前浏览器不支持流式响应');

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let result: SolveResponse | null = null;

  const handleFrame = (frame: string) => {
    const parsed = parseSseFrame(frame);
    if (!parsed) return;
    if (parsed.event === 'progress') {
      if (onProgress) {
        try {
          onProgress(JSON.parse(parsed.data) as SolveProgress);
        } catch {
          /* ignore malformed progress frame */
        }
      }
    } else if (parsed.event === 'result') {
      result = JSON.parse(parsed.data) as SolveResponse;
    } else if (parsed.event === 'error') {
      throw new Error(parsed.data || '求解失败');
    }
  };

  try {
    let streaming = true;
    while (streaming) {
      const { done, value } = await reader.read();
      if (value) buffer += decoder.decode(value, { stream: true });
      // Match the frame separator directly (`\n\n`, or `\r\n\r\n` if a proxy
      // rewrote line endings) instead of rewriting the whole buffer each chunk —
      // a per-chunk `buffer.replace` would be O(N²) for a large final frame.
      let match = buffer.match(/\r?\n\r?\n/);
      while (match) {
        const sep = match.index ?? 0;
        handleFrame(buffer.slice(0, sep));
        buffer = buffer.slice(sep + match[0].length);
        match = buffer.match(/\r?\n\r?\n/);
      }
      if (done) {
        streaming = false;
      }
    }
    // Flush any trailing frame that wasn't newline-terminated.
    if (buffer.trim().length > 0) handleFrame(buffer);
  } catch (err) {
    // handleFrame may throw (error event / malformed result JSON); cancel the
    // stream so the connection isn't leaked, then rethrow.
    reader.cancel().catch(() => {});
    throw err;
  } finally {
    reader.releaseLock();
  }

  if (!result) throw new Error('求解结束但未返回结果');
  return result;
}

// Minimal typings for the File System Access API (not in the default DOM lib
// for this toolchain). Only the members we use are declared.
interface SaveFilePickerOptions {
  suggestedName?: string;
  types?: { description?: string; accept: Record<string, string[]> }[];
}
interface WritableFileStreamLike {
  // Accept `Uint8Array` directly: the response reader yields
  // `Uint8Array<ArrayBufferLike>`, which TS 6's `BufferSource` (an
  // `ArrayBufferView<ArrayBuffer>`) rejects over the SharedArrayBuffer case.
  write(data: Uint8Array | Blob | string): Promise<void>;
  close(): Promise<void>;
}
interface FileHandleLike {
  createWritable(): Promise<WritableFileStreamLike>;
}
type ShowSaveFilePicker = (options?: SaveFilePickerOptions) => Promise<FileHandleLike>;

/** Trigger a browser download of `blob` under `filename`. */
function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/** Outcome of {@link solveStreamToFile}. */
export interface StreamToFileResult {
  /** Number of solutions written (one NDJSON line each). */
  count: number;
  /** True if streamed directly to disk; false if buffered then downloaded
   * (fallback for browsers without the File System Access API). */
  streamedToDisk: boolean;
}

/**
 * Stream every solution from the backend straight into a file as NDJSON,
 * without holding the full set in memory (issue #21).
 *
 * Prefers the File System Access API (`showSaveFilePicker`) so bytes go to disk
 * as they arrive; falls back to buffering + a normal download where it is
 * unavailable. `onCount` reports the running solution count for progress.
 *
 * Must be called from a user gesture (the save dialog requires user activation).
 */
export async function solveStreamToFile(
  length: number,
  rows: SolveRequest['rows'],
  options: SolveOptions = {},
  onCount?: (count: number) => void,
): Promise<StreamToFileResult> {
  // The streaming endpoint and the File System Access API are browser-only.
  // In the standalone app, file export will go through the Tauri dialog/fs
  // plugins (planned); fail clearly for now instead of hitting a missing server.
  if (isTauri()) {
    throw new Error('文件导出暂未在 App 中支持');
  }
  const suggestedName = `sumzle_solutions_${Math.floor(Date.now() / 1000)}.ndjson`;

  // Open the save dialog first, while we still have user activation.
  let writable: WritableFileStreamLike | null = null;
  const picker = (window as unknown as { showSaveFilePicker?: ShowSaveFilePicker })
    .showSaveFilePicker;
  if (picker) {
    try {
      const handle = await picker({
        suggestedName,
        types: [{ description: 'NDJSON', accept: { 'application/x-ndjson': ['.ndjson'] } }],
      });
      writable = await handle.createWritable();
    } catch (e) {
      // User dismissed the picker — abort without firing the solve request.
      if (e instanceof DOMException && e.name === 'AbortError') {
        throw new Error('已取消保存', { cause: e });
      }
      writable = null; // any other failure → fall back to a buffered download
    }
  }

  // From here the writable (if any) is open, so guard the request + streaming
  // loop with try/catch/finally: on any error mid-stream we must still cancel
  // the response reader and close the writable, or the connection and the OS
  // file handle leak (leaving the file locked/incomplete).
  let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  try {
    const res = await fetch(`${API_BASE}/solve/stream?${solveParams(options, false)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ length, rows }),
    });
    if (!res.ok) throw new Error(await res.text());
    if (!res.body) throw new Error('当前浏览器不支持流式响应');

    reader = res.body.getReader();
    const decoder = new TextDecoder();
    const chunks: Uint8Array[] = [];
    let count = 0;
    let tail = '';

    let streaming = true;
    while (streaming) {
      const { done, value } = await reader.read();
      if (value) {
        if (writable) {
          await writable.write(value);
        } else {
          chunks.push(value);
        }
        // Each solution is one '\n'-terminated NDJSON line; count completed lines.
        const text = tail + decoder.decode(value, { stream: true });
        const lines = text.split('\n');
        tail = lines.pop() ?? '';
        count += lines.length;
        if (onCount) onCount(count);
      }
      if (done) streaming = false;
    }
    if (tail.trim().length > 0) {
      count += 1;
      if (onCount) onCount(count);
    }

    if (writable) {
      await writable.close();
      writable = null; // closed cleanly — don't double-close in `finally`
      return { count, streamedToDisk: true };
    }
    downloadBlob(new Blob(chunks as BlobPart[], { type: 'application/x-ndjson' }), suggestedName);
    return { count, streamedToDisk: false };
  } catch (err) {
    // Cancel the response stream so the connection isn't leaked on error.
    if (reader) reader.cancel().catch(() => {});
    throw err;
  } finally {
    if (reader) reader.releaseLock();
    // Reached with an open writable only on an error path; release the handle.
    if (writable) {
      try {
        await writable.close();
      } catch {
        // Ignore secondary errors while cleaning up.
      }
    }
  }
}
