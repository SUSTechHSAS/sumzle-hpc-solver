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
});
