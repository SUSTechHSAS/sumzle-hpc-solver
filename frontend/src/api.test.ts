import { describe, it, expect, vi, beforeEach } from 'vitest';
import { solvePuzzle, validateEquation, evaluateExpression } from './api';

describe('API functions', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('solvePuzzle', () => {
    it('sends correct request and returns data', async () => {
      const mockResponse = {
        solutions: ['1+2=3'],
        stats: { searched_count: 100, found_count: 1, elapsed_ms: 5, speed: 20000 },
      };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      } as Response);

      const result = await solvePuzzle(5, [{ tiles: [{ char: '1', state: 'correct' }] }], 2);
      expect(fetch).toHaveBeenCalledWith('/api/solve?threads=2', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          length: 5,
          rows: [{ tiles: [{ char: '1', state: 'correct' }] }],
        }),
      });
      expect(result).toEqual(mockResponse);
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

  describe('solvePuzzle JSON body structure', () => {
    // These tests verify that the JSON body sent by the frontend matches
    // the format expected by the Rust backend's serde deserialization.
    // This prevents the bug where the frontend sent {rows: [{tiles: [...]}]}
    // but the backend expected {rows: [[...]]}.

    it('sends rows with tiles wrapper (not raw arrays)', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ solutions: [], stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 } }),
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
        json: () => Promise.resolve({ solutions: [], stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 } }),
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
        json: () => Promise.resolve({ solutions: [], stats: { searched_count: 0, found_count: 0, elapsed_ms: 0, speed: 0 } }),
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
