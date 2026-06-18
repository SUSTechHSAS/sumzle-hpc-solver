import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  solvePuzzle,
  validateEquation,
  evaluateExpression,
  downloadResults,
  solveWithProgress,
  solveStreamToFile,
} from './api';
import type { SolveProgress } from './types';

/** Build a ReadableStream that emits the given strings as UTF-8 chunks. */
function streamOf(...chunks: string[]): ReadableStream<Uint8Array> {
  const enc = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const c of chunks) controller.enqueue(enc.encode(c));
      controller.close();
    },
  });
}

describe('API functions', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('solvePuzzle', () => {
    it('sends correct request and returns data with new fields', async () => {
      const mockResponse = {
        solutions: ['1+2=3'],
        stats: { searched_count: 100, found_count: 1, elapsed_ms: 5, speed: 20000 },
        char_probabilities: [
          { char: '=', display: '=', count: 1, probability: 100 },
          { char: '1', display: '1', count: 1, probability: 100 },
        ],
        recommended: '1+2=3',
        top: 0,
        scores: [],
      };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      const result = await solvePuzzle(5, [{ tiles: [{ char: '1', state: 'correct' }] }], {
        threads: 2,
      });
      expect(fetch).toHaveBeenCalledWith('/api/solve?threads=2', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          length: 5,
          rows: [{ tiles: [{ char: '1', state: 'correct' }] }],
        }),
      });
      expect(result).toEqual(mockResponse);
      expect(result.char_probabilities).toBeDefined();
      expect(result.recommended).toBe('1+2=3');
    });

    it('adds the top parameter to the URL when requesting top-N', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            solutions: ['1+2=3'],
            stats: { searched_count: 100, found_count: 1, elapsed_ms: 5, speed: 20000 },
            char_probabilities: [],
            recommended: '1+2=3',
            top: 3,
            scores: [245.5],
          }),
      } as Response);

      await solvePuzzle(5, [], { threads: 4, top: 3 });
      expect(fetch).toHaveBeenCalledWith('/api/solve?threads=4&top=3', expect.any(Object));
    });

    it('omits the top parameter when top is 0', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () =>
          Promise.resolve({
            solutions: [],
            stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 },
            char_probabilities: [],
            recommended: null,
            top: 0,
            scores: [],
          }),
      } as Response);

      await solvePuzzle(5, [], { top: 0 });
      expect(fetch).toHaveBeenCalledWith('/api/solve?threads=0', expect.any(Object));
    });

    it('throws on non-ok response', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        text: () => Promise.resolve('Server error'),
      } as Response);

      await expect(solvePuzzle(5, [])).rejects.toThrow('Server error');
    });
  });

  describe('validateEquation', () => {
    it('sends correct request and returns data', async () => {
      const mockResponse = { valid: true };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      const result = await validateEquation('1+2=3');
      expect(fetch).toHaveBeenCalledWith('/api/validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ equation: '1+2=3' }),
      });
      expect(result).toEqual(mockResponse);
    });

    it('throws on non-ok response', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        text: () => Promise.resolve('Bad request'),
      } as Response);

      await expect(validateEquation('invalid')).rejects.toThrow('Bad request');
    });
  });

  describe('evaluateExpression', () => {
    it('sends correct request and returns data', async () => {
      const mockResponse = { result: '120' };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      const result = await evaluateExpression('5!');
      expect(fetch).toHaveBeenCalledWith('/api/eval', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ expression: '5!' }),
      });
      expect(result).toEqual(mockResponse);
    });

    it('throws on non-ok response', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        text: () => Promise.resolve('Parse error'),
      } as Response);

      await expect(evaluateExpression('???')).rejects.toThrow('Parse error');
    });
  });

  describe('downloadResults', () => {
    const mockData: import('./types').SolveResponse = {
      solutions: ['1+2=3', '2+3=5'],
      stats: { searched_count: 100, found_count: 2, elapsed_ms: 5, speed: 20000 },
      char_probabilities: [],
      recommended: '1+2=3',
      top: 0,
      scores: [],
    };

    it('generates JSON download content', () => {
      // Mock URL.createObjectURL and related DOM APIs
      const mockUrl = 'blob:mock-url';
      const mockAnchor = {
        href: '', download: '', click: vi.fn(), style: {},
        setAttribute: vi.fn(), removeAttribute: vi.fn(),
      } as unknown as HTMLAnchorElement;
      vi.spyOn(document, 'createElement').mockReturnValueOnce(mockAnchor);
      vi.spyOn(URL, 'createObjectURL').mockReturnValueOnce(mockUrl);
      vi.spyOn(URL, 'revokeObjectURL').mockImplementationOnce(() => {});
      vi.spyOn(document.body, 'appendChild').mockImplementationOnce(() => mockAnchor);
      vi.spyOn(document.body, 'removeChild').mockImplementationOnce(() => mockAnchor);

      downloadResults(mockData, 'json');
      expect(mockAnchor.click).toHaveBeenCalled();
      expect(mockAnchor.download).toMatch(/\.json$/);
    });

    it('generates CSV download content', () => {
      const mockUrl = 'blob:mock-url';
      const mockAnchor = {
        href: '', download: '', click: vi.fn(), style: {},
        setAttribute: vi.fn(), removeAttribute: vi.fn(),
      } as unknown as HTMLAnchorElement;
      vi.spyOn(document, 'createElement').mockReturnValueOnce(mockAnchor);
      vi.spyOn(URL, 'createObjectURL').mockReturnValueOnce(mockUrl);
      vi.spyOn(URL, 'revokeObjectURL').mockImplementationOnce(() => {});
      vi.spyOn(document.body, 'appendChild').mockImplementationOnce(() => mockAnchor);
      vi.spyOn(document.body, 'removeChild').mockImplementationOnce(() => mockAnchor);

      downloadResults(mockData, 'csv');
      expect(mockAnchor.click).toHaveBeenCalled();
      expect(mockAnchor.download).toMatch(/\.csv$/);
    });

    it('generates TXT download content', () => {
      const mockUrl = 'blob:mock-url';
      const mockAnchor = {
        href: '', download: '', click: vi.fn(), style: {},
        setAttribute: vi.fn(), removeAttribute: vi.fn(),
      } as unknown as HTMLAnchorElement;
      vi.spyOn(document, 'createElement').mockReturnValueOnce(mockAnchor);
      vi.spyOn(URL, 'createObjectURL').mockReturnValueOnce(mockUrl);
      vi.spyOn(URL, 'revokeObjectURL').mockImplementationOnce(() => {});
      vi.spyOn(document.body, 'appendChild').mockImplementationOnce(() => mockAnchor);
      vi.spyOn(document.body, 'removeChild').mockImplementationOnce(() => mockAnchor);

      downloadResults(mockData, 'txt');
      expect(mockAnchor.click).toHaveBeenCalled();
      expect(mockAnchor.download).toMatch(/\.txt$/);
    });
  });

  describe('solvePuzzle JSON body structure', () => {
    it('sends rows with tiles wrapper (not raw arrays)', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          solutions: [],
          stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 },
          char_probabilities: [],
          recommended: null,
        }),
      } as Response);

      await solvePuzzle(6, [
        { tiles: [{ char: '1', state: 'correct' }, { char: '+', state: 'present' }] },
      ]);

      const call = (fetch as ReturnType<typeof vi.spyOn>).mock.calls[0];
      const body = JSON.parse(call[1].body as string);

      // rows must be an array of objects with a "tiles" key
      expect(body.rows).toBeInstanceOf(Array);
      expect(body.rows[0]).toHaveProperty('tiles');
      expect(body.rows[0].tiles).toBeInstanceOf(Array);
      // rows[0] must NOT be a bare array (this was the bug)
      expect(Array.isArray(body.rows[0]) && !('tiles' in body.rows[0])).toBe(false);
    });

    it('sends tiles with char and state string fields', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          solutions: [],
          stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 },
          char_probabilities: [],
          recommended: null,
        }),
      } as Response);

      await solvePuzzle(6, [
        { tiles: [{ char: '1', state: 'correct' }] },
      ]);

      const call = (fetch as ReturnType<typeof vi.spyOn>).mock.calls[0];
      const body = JSON.parse(call[1].body as string);

      const tile = body.rows[0].tiles[0];
      expect(tile).toHaveProperty('char');
      expect(tile).toHaveProperty('state');
      expect(typeof tile.state).toBe('string');
      expect(['correct', 'present', 'empty']).toContain(tile.state);
    });

    it('sends multiple rows correctly', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          solutions: [],
          stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 },
          char_probabilities: [],
          recommended: null,
        }),
      } as Response);

      await solvePuzzle(6, [
        { tiles: [{ char: '1', state: 'correct' }] },
        { tiles: [{ char: '2', state: 'present' }] },
      ]);

      const call = (fetch as ReturnType<typeof vi.spyOn>).mock.calls[0];
      const body = JSON.parse(call[1].body as string);

      expect(body.rows.length).toBe(2);
      expect(body.rows[0].tiles[0].char).toBe('1');
      expect(body.rows[1].tiles[0].char).toBe('2');
    });
  });

  describe('solveWithProgress', () => {
    it('parses SSE progress + result frames', async () => {
      const result = {
        solutions: ['1+2=3'],
        stats: { searched_count: 100, found_count: 6243, elapsed_ms: 5, speed: 20000 },
        char_probabilities: [],
        recommended: '1+2=3',
        top: 0,
        scores: [],
      };
      const sse =
        'event: progress\ndata: {"done":1,"total":4,"phase":1}\n\n' +
        'event: progress\ndata: {"done":4,"total":4,"phase":3}\n\n' +
        `event: result\ndata: ${JSON.stringify(result)}\n\n`;
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        body: streamOf(sse),
      } as unknown as Response);

      const seen: SolveProgress[] = [];
      const got = await solveWithProgress(5, [], { threads: 2 }, (p) => seen.push(p));

      expect(fetch).toHaveBeenCalledWith('/api/solve/progress?threads=2', expect.any(Object));
      expect(seen).toEqual([
        { done: 1, total: 4, phase: 1 },
        { done: 4, total: 4, phase: 3 },
      ]);
      expect(got.solutions).toEqual(['1+2=3']);
      expect(got.stats.found_count).toBe(6243);
    });

    it('reassembles frames split across chunks and sends top in the URL', async () => {
      const result = {
        solutions: [],
        stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 },
        char_probabilities: [],
        recommended: null,
        top: 3,
        scores: [],
      };
      const full = `event: result\ndata: ${JSON.stringify(result)}\n\n`;
      const mid = Math.floor(full.length / 2);
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        body: streamOf(full.slice(0, mid), full.slice(mid)),
      } as unknown as Response);

      const got = await solveWithProgress(6, [], { threads: 0, top: 3 });
      expect(fetch).toHaveBeenCalledWith(
        '/api/solve/progress?threads=0&top=3',
        expect.any(Object),
      );
      expect(got.top).toBe(3);
    });

    it('throws on a non-ok response', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        text: () => Promise.resolve('bad length'),
      } as Response);
      await expect(solveWithProgress(2, [])).rejects.toThrow('bad length');
    });
  });

  describe('solveStreamToFile', () => {
    it('falls back to a blob download when the file picker is unavailable', async () => {
      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = undefined;
      const ndjson = '{"solution":"1+2=3"}\n{"solution":"2+1=3"}\n';
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        body: streamOf(ndjson),
      } as unknown as Response);

      const mockAnchor = {
        href: '',
        download: '',
        click: vi.fn(),
        style: {},
        setAttribute: vi.fn(),
        removeAttribute: vi.fn(),
      } as unknown as HTMLAnchorElement;
      vi.spyOn(document, 'createElement').mockReturnValueOnce(mockAnchor);
      vi.spyOn(URL, 'createObjectURL').mockReturnValueOnce('blob:mock');
      vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
      vi.spyOn(document.body, 'appendChild').mockImplementation(() => mockAnchor);
      vi.spyOn(document.body, 'removeChild').mockImplementation(() => mockAnchor);

      const counts: number[] = [];
      const r = await solveStreamToFile(5, [], { threads: 1 }, (c) => counts.push(c));

      expect(fetch).toHaveBeenCalledWith('/api/solve/stream?threads=1', expect.any(Object));
      expect(r.streamedToDisk).toBe(false);
      expect(r.count).toBe(2);
      expect(counts.at(-1)).toBe(2);
      expect(mockAnchor.click).toHaveBeenCalled();
      expect(mockAnchor.download).toMatch(/\.ndjson$/);
    });

    it('streams directly to disk when showSaveFilePicker is available', async () => {
      const written: Uint8Array[] = [];
      const writable = {
        write: vi.fn((d: Uint8Array) => {
          written.push(d);
          return Promise.resolve();
        }),
        close: vi.fn(() => Promise.resolve()),
      };
      const handle = { createWritable: vi.fn(() => Promise.resolve(writable)) };
      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = vi.fn(() =>
        Promise.resolve(handle),
      );
      const ndjson = '{"solution":"1+2=3"}\n{"solution":"2+1=3"}\n{"solution":"3+0=3"}\n';
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        body: streamOf(ndjson),
      } as unknown as Response);

      const r = await solveStreamToFile(5, [], { threads: 0 });

      expect(fetch).toHaveBeenCalledWith('/api/solve/stream?threads=0', expect.any(Object));
      expect(r.streamedToDisk).toBe(true);
      expect(r.count).toBe(3);
      expect(writable.write).toHaveBeenCalled();
      expect(writable.close).toHaveBeenCalled();

      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = undefined;
    });

    it('closes the writable if the request fails mid-stream', async () => {
      const writable = {
        write: vi.fn(() => Promise.resolve()),
        close: vi.fn(() => Promise.resolve()),
      };
      const handle = { createWritable: vi.fn(() => Promise.resolve(writable)) };
      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = vi.fn(() =>
        Promise.resolve(handle),
      );
      // Server rejects the request after the writable was already opened.
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        text: () => Promise.resolve('boom'),
      } as Response);

      await expect(solveStreamToFile(5, [], { threads: 0 })).rejects.toThrow('boom');
      // The file handle must be released even though streaming never started.
      expect(writable.close).toHaveBeenCalled();

      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = undefined;
    });

    it('throws "已取消保存" and never fetches when the save dialog is cancelled', async () => {
      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = vi.fn(() =>
        Promise.reject(new DOMException('The user aborted a request.', 'AbortError')),
      );
      const fetchSpy = vi.spyOn(globalThis, 'fetch');

      await expect(solveStreamToFile(5, [], { threads: 0 })).rejects.toThrow('已取消保存');
      expect(fetchSpy).not.toHaveBeenCalled();

      (window as { showSaveFilePicker?: unknown }).showSaveFilePicker = undefined;
    });
  });

});
