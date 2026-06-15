import { describe, it, expect, vi, beforeEach } from 'vitest';
import { solvePuzzle, validateEquation, evaluateExpression, downloadResults } from './api';

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
      };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      const result = await solvePuzzle(5, [{ tiles: [{ char: '1', state: 'correct' }] }], { threads: 2 });
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
      expect(result.top).toBe(0);
    });

    it('sends top-N parameter when provided', async () => {
      const mockResponse = {
        solutions: ['1+2=3'],
        stats: { searched_count: 100, found_count: 1, elapsed_ms: 5, speed: 20000 },
        char_probabilities: [],
        recommended: '1+2=3',
        top: 5,
      };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      const result = await solvePuzzle(5, [], { threads: 0, top: 5 });
      expect(fetch).toHaveBeenCalledWith('/api/solve?threads=0&top=5', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ length: 5, rows: [] }),
      });
      expect(result.top).toBe(5);
    });

    it('does not send top parameter when top is 0', async () => {
      const mockResponse = {
        solutions: [],
        stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 },
        char_probabilities: [],
        recommended: null,
        top: 0,
      };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      await solvePuzzle(5, [], { threads: 0, top: 0 });
      const call = (fetch as ReturnType<typeof vi.spyOn>).mock.calls[0];
      const url = call[0] as string;
      expect(url).toBe('/api/solve?threads=0');
      expect(url).not.toContain('top');
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
          top: 0,
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
          top: 0,
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
          top: 0,
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

});
