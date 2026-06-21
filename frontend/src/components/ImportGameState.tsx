import { useState, useRef } from 'react';
import type { GuessRow, TileState } from '../types';
import { MIN_LENGTH, MAX_LENGTH } from '../utils';
import Icon from './Icon';
import './ImportGameState.css';

interface ImportGameStateProps {
  length: number;
  onImport: (length: number, rows: GuessRow[]) => void;
}

interface SumzleRow {
  length?: number;
  rows?: unknown[];
}

export default function ImportGameState({ length, onImport }: ImportGameStateProps) {
  const [showImport, setShowImport] = useState(false);
  const [importText, setImportText] = useState('');
  const [importError, setImportError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleImportPaste = () => {
    if (!importText.trim()) return;
    setImportError(null);
    try {
      const data = JSON.parse(importText.trim());
      parseAndImport(data);
      setImportText('');
      setShowImport(false);
    } catch (e) {
      setImportError(e instanceof Error ? e.message : '无效的JSON格式');
    }
  };

  const handleFileImport = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      try {
        const data = JSON.parse(ev.target?.result as string);
        parseAndImport(data);
      } catch (err) {
        setImportError(err instanceof Error ? err.message : '无效的JSON文件');
      }
    };
    reader.readAsText(file);
    // Reset file input
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  const parseAndImport = (data: SumzleRow) => {
    // Support format: { length, rows: [[ {char, state} ]] }
    if (!data.rows || !Array.isArray(data.rows)) {
      throw new Error('JSON格式错误: 缺少rows数组');
    }

    const importedLength = data.length || length;
    // Reject out-of-range lengths BEFORE iterating the rows. A malicious or
    // buggy JSON payload could declare length=1,000,000 with matching tile
    // arrays; building those tile objects and then rendering them would
    // freeze the browser (issue #29).
    if (
      !Number.isFinite(importedLength) ||
      importedLength < MIN_LENGTH ||
      importedLength > MAX_LENGTH
    ) {
      throw new Error(
        `表达式长度 ${importedLength} 超出支持范围，请输入 ${MIN_LENGTH}–${MAX_LENGTH} 之间的整数`,
      );
    }
    const rows: GuessRow[] = data.rows.map((row: unknown, rowIndex: number) => {
      if (!Array.isArray(row)) {
        throw new Error(`JSON格式错误: rows中的第${rowIndex + 1}行必须是数组`);
      }
      const tiles = row.map((tile: unknown, tileIndex: number) => {
        if (!tile || typeof tile !== 'object') {
          throw new Error(`JSON格式错误: 第${rowIndex + 1}行第${tileIndex + 1}个tile必须是对象`);
        }
        const t = tile as { char?: unknown; state?: unknown };
        return {
          char: t.char !== undefined && t.char !== null ? String(t.char) : '',
          state: normalizeState(t.state !== undefined && t.state !== null ? String(t.state) : ''),
        };
      });
      if (tiles.length !== importedLength) {
        throw new Error(`JSON格式错误: 第${rowIndex + 1}行的列数(${tiles.length})与表达式长度(${importedLength})不匹配`);
      }
      return { tiles };
    });

    onImport(importedLength, rows);
  };

  const normalizeState = (state: string): TileState => {
    const s = state.toLowerCase();
    if (s === 'correct' || s === 'green' || s === 'g') return 'correct';
    if (s === 'present' || s === 'yellow' || s === 'y') return 'present';
    return 'empty';
  };

  return (
    <div className="import-section">
      <button
        className="btn btn-import-toggle"
        onClick={() => setShowImport(!showImport)}
      >
        <Icon name={showImport ? 'chevron-up' : 'download'} />
        {showImport ? '收起导入' : '导入局面'}
      </button>

      {showImport && (
        <div className="import-panel">
          <textarea
            className="import-textarea"
            placeholder="粘贴 Sumzle 游戏状态 JSON"
            value={importText}
            onChange={(e) => setImportText(e.target.value)}
            rows={4}
          />
          <div className="import-actions">
            <button className="btn btn-primary btn-sm" onClick={handleImportPaste}>
              导入JSON
            </button>
            <button
              className="btn btn-secondary btn-sm"
              onClick={() => fileInputRef.current?.click()}
            >
              <Icon name="file" />
              从文件导入
            </button>
            <input
              ref={fileInputRef}
              type="file"
              accept=".json"
              style={{ display: 'none' }}
              onChange={handleFileImport}
            />
          </div>
          {importError && (
            <div className="import-error" role="alert">
              {importError}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
