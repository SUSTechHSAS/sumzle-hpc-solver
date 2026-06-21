import { describe, it, expect } from 'vitest';
import { displayChar, CHAR_DISPLAY, VALID_CHARS } from './types';
import {
  createBlankRow,
  cycleState,
  formatSpeed,
  formatTime,
  clampLength,
  MIN_LENGTH,
  MAX_LENGTH,
} from './utils';

describe('types', () => {
  it('should have correct display char mappings', () => {
    expect(displayChar('*')).toBe('×');
    expect(displayChar('/')).toBe('÷');
    expect(displayChar('+')).toBe('+');
    expect(displayChar('1')).toBe('1');
    expect(displayChar('=')).toBe('=');
  });

  it('should have valid character rows for keyboard', () => {
    expect(VALID_CHARS.length).toBe(3);
    expect(VALID_CHARS[0]).toContain('1');
    expect(VALID_CHARS[0]).toContain('0');
    expect(VALID_CHARS[1]).toContain('+');
    expect(VALID_CHARS[1]).toContain('=');
    expect(VALID_CHARS[2]).toContain('A');
    expect(VALID_CHARS[2]).toContain('⌫');
  });

  it('should have CHAR_DISPLAY for * and /', () => {
    expect(CHAR_DISPLAY['*']).toBe('×');
    expect(CHAR_DISPLAY['/']).toBe('÷');
  });
});

describe('utils', () => {
  it('should create blank rows with correct length', () => {
    const row = createBlankRow(5);
    expect(row.tiles.length).toBe(5);
    row.tiles.forEach((tile) => {
      expect(tile.char).toBe('');
      expect(tile.state).toBe('empty');
    });
  });

  it('should cycle states correctly', () => {
    expect(cycleState('correct')).toBe('present');
    expect(cycleState('present')).toBe('empty');
    expect(cycleState('empty')).toBe('correct');
  });

  it('should format speed correctly', () => {
    expect(formatSpeed(500)).toBe('500');
    expect(formatSpeed(1500)).toBe('2K');
    expect(formatSpeed(1500000)).toBe('1.5M');
  });

  it('should format time correctly', () => {
    expect(formatTime(50)).toBe('50ms');
    expect(formatTime(1500)).toBe('1.50s');
    expect(formatTime(90000)).toBe('1m 30.0s');
  });
});

describe('clampLength', () => {
  it('exposes a sensible minimum and maximum', () => {
    expect(MIN_LENGTH).toBeGreaterThanOrEqual(3);
    expect(MAX_LENGTH).toBeGreaterThan(MIN_LENGTH);
    // MAX_LENGTH must be small enough that rendering that many tiles per row
    // does not freeze the browser (issue #29).
    expect(MAX_LENGTH).toBeLessThanOrEqual(256);
  });

  it('clamps below MIN_LENGTH up to MIN_LENGTH', () => {
    expect(clampLength(0)).toBe(MIN_LENGTH);
    expect(clampLength(-5)).toBe(MIN_LENGTH);
    expect(clampLength(2)).toBe(MIN_LENGTH);
  });

  it('clamps above MAX_LENGTH down to MAX_LENGTH', () => {
    expect(clampLength(MAX_LENGTH + 1)).toBe(MAX_LENGTH);
    expect(clampLength(100)).toBe(MAX_LENGTH);
    expect(clampLength(1_000_000)).toBe(MAX_LENGTH);
    expect(clampLength(Number.MAX_SAFE_INTEGER)).toBe(MAX_LENGTH);
  });

  it('passes through in-range values, truncating any fractional part', () => {
    expect(clampLength(MIN_LENGTH)).toBe(MIN_LENGTH);
    expect(clampLength(5)).toBe(5);
    expect(clampLength(MAX_LENGTH)).toBe(MAX_LENGTH);
    expect(clampLength(5.9)).toBe(5);
  });

  it('returns MIN_LENGTH for non-finite values (defensive)', () => {
    expect(clampLength(NaN)).toBe(MIN_LENGTH);
    expect(clampLength(Infinity)).toBe(MIN_LENGTH);
    expect(clampLength(-Infinity)).toBe(MIN_LENGTH);
  });
});
