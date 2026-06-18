import { VALID_CHARS } from '../types';
import './VirtualKeyboard.css';

interface VirtualKeyboardProps {
  onKeyPress: (key: string) => void;
}

export default function VirtualKeyboard({ onKeyPress }: VirtualKeyboardProps) {
  return (
    <div className="virtual-keyboard">
      {VALID_CHARS.map((row, i) => (
        <div key={i} className="keyboard-row">
          {row.map((key) => (
            <button
              key={key}
              className={`keyboard-key${key === '⌫' ? ' key-backspace' : ''}`}
              onClick={() => onKeyPress(key)}
              aria-label={key === '⌫' ? '删除当前字符' : `输入 ${key}`}
            >
              {key}
            </button>
          ))}
        </div>
      ))}
    </div>
  );
}
