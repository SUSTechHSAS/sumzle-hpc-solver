import { describe, it, expect } from 'vitest';
import { displayChar, CHAR_DISPLAY, VALID_CHARS } from './types';
import { createBlankRow, cycleState, formatSpeed, formatTime } from './utils';

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
