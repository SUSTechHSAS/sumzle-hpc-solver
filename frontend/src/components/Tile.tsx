import { useRef, useEffect } from 'react';
import { type Tile as TileType, type TileState, VALID_CHARS } from '../types';
import './Tile.css';

interface TileProps {
  tile: TileType;
  onCharChange: (char: string) => void;
  onStateToggle: () => void;
  selected?: boolean;
  onSelect?: () => void;
}

const STATE_LABELS: Record<TileState, string> = {
  correct: '✓',
  present: '●',
  empty: '✕',
};

const STATE_COPY: Record<TileState, { label: string; next: string }> = {
  correct: { label: '正确位置', next: '错位存在' },
  present: { label: '错位存在', next: '不存在' },
  empty: { label: '不存在', next: '正确位置' },
};

const VALID_DIRECT_INPUT = new Set(VALID_CHARS.flat().filter((key) => key !== '⌫'));
const DIRECT_INPUT_ALIASES: Record<string, string> = {
  '×': '*',
  '÷': '/',
};

function normalizeDirectInput(value: string): string {
  const raw = value.slice(-1).toUpperCase();
  const normalized = DIRECT_INPUT_ALIASES[raw] ?? raw;
  return VALID_DIRECT_INPUT.has(normalized) ? normalized : '';
}

export default function Tile({ tile, onCharChange, onStateToggle, selected, onSelect }: TileProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const stateCopy = STATE_COPY[tile.state];

  useEffect(() => {
    if (selected && inputRef.current) {
      inputRef.current.focus();
    }
  }, [selected]);

  const handleInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    onCharChange(val.length === 0 ? '' : normalizeDirectInput(val));
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Backspace' && tile.char === '') {
      onCharChange('');
    }
  };

  return (
    <div
      className={`tile tile-${tile.state}${selected ? ' tile-selected' : ''}`}
      onClick={onSelect}
    >
      <input
        ref={inputRef}
        className="tile-input"
        type="text"
        maxLength={1}
        value={tile.char}
        onChange={handleInput}
        onKeyDown={handleKeyDown}
        aria-label="输入方块字符"
        onClick={(e) => e.stopPropagation()}
      />
      <button
        className={`tile-state-btn tile-state-${tile.state}`}
        onClick={(e) => {
          e.stopPropagation();
          onStateToggle();
        }}
        title={`当前标记：${stateCopy.label}。点击切换为：${stateCopy.next}。`}
        aria-label={`当前标记：${stateCopy.label}。点击切换为：${stateCopy.next}。`}
      >
        {STATE_LABELS[tile.state]}
      </button>
    </div>
  );
}

export { STATE_LABELS };
