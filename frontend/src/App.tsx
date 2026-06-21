import { useState, useCallback, useRef } from 'react';
import type { GuessRow, TileState, DownloadFormat, SolveResponse } from './types';
import { solvePuzzle, downloadResults } from './api';
import GuessRowComponent from './components/GuessRow';
import VirtualKeyboard from './components/VirtualKeyboard';
import ImportGameState from './components/ImportGameState';
import { createBlankRow, cycleState, clampLength, MIN_LENGTH, MAX_LENGTH } from './utils';
import Results from './components/Results';
import ExpressionEvaluator from './components/ExpressionEvaluator';
import EquationValidator from './components/EquationValidator';
import Icon from './components/Icon';
import './App.css';

const DEFAULT_LENGTH = 5;

/**
 * Resize a guess row to `len` tiles, padding with blanks or truncating as
 * needed. Uses `slice` for the truncation step so shrinking a row that was
 * somehow created with a huge tile count is O(len) rather than O(prev_len).
 */
const adjustRowLength = (row: GuessRow, len: number): GuessRow => {
  const tiles = row.tiles.slice(0, len);
  while (tiles.length < len) {
    tiles.push({ char: '', state: 'empty' as TileState });
  }
  return { tiles };
};

export default function App() {
  const [darkMode, setDarkMode] = useState(false);
  const [length, setLength] = useState(DEFAULT_LENGTH);
  const [lengthDraft, setLengthDraft] = useState(String(DEFAULT_LENGTH));
  const [lengthError, setLengthError] = useState<string | null>(null);
  const [rows, setRows] = useState<GuessRow[]>([createBlankRow(DEFAULT_LENGTH)]);
  const [solutions, setSolutions] = useState<SolveResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedTile, setSelectedTile] = useState<{ row: number; col: number } | null>(null);
  // Solver options: threads (0 = auto) and top-N (0 = return every solution).
  const [threads, setThreads] = useState(0);
  const [topN, setTopN] = useState(0);

  const commitLength = useCallback((newLength: number) => {
    // Always clamp to the supported range. The backend supports arbitrary
    // expression lengths, but the frontend must not render an unbounded
    // number of guess boxes — doing so freezes the browser (issue #29).
    const clamped = clampLength(newLength);
    setLength(clamped);
    setLengthDraft(String(clamped));
    setLengthError(null);
    setRows((prev) => prev.map((row) => adjustRowLength(row, clamped)));
    setSolutions(null);
    setError(null);
  }, []);

  const handleLengthDraftChange = useCallback(
    (value: string) => {
      setLengthDraft(value);
      const parsed = Number.parseInt(value, 10);
      if (!Number.isFinite(parsed)) {
        // Empty input or non-numeric — let the user keep typing without
        // committing. No error message: this is a normal intermediate state.
        setLengthError(null);
        return;
      }
      if (parsed < MIN_LENGTH) {
        // Likely an intermediate keystroke (e.g. typing "1" on the way to
        // "15"). Don't commit and don't warn — committing would shrink the
        // board to MIN_LENGTH and discard the user's in-progress value.
        setLengthError(null);
        return;
      }
      if (parsed > MAX_LENGTH) {
        // Definitely out of range. Do NOT commit: committing would trigger
        // `setRows` which would allocate a huge tile array per row and
        // freeze the browser. Surface an error so the user knows their
        // typed value will not be honored as-is.
        setLengthError(`表达式长度不能超过 ${MAX_LENGTH}，请输入 ${MIN_LENGTH}–${MAX_LENGTH} 之间的整数。`);
        return;
      }
      setLengthError(null);
      commitLength(parsed);
    },
    [commitLength],
  );

  const handleLengthBlur = useCallback(() => {
    const parsed = Number.parseInt(lengthDraft, 10);
    // `commitLength` clamps internally, so an out-of-range value typed by the
    // user gets normalized to the nearest bound on blur (e.g. 1,000,000 →
    // MAX_LENGTH), keeping the rendered tile count safe.
    commitLength(Number.isFinite(parsed) ? parsed : length);
  }, [commitLength, length, lengthDraft]);

  const handleTileCharChange = useCallback(
    (rowIndex: number, tileIndex: number, char: string) => {
      setRows((prev) => {
        const newRows = [...prev];
        const newRow = { ...newRows[rowIndex], tiles: [...newRows[rowIndex].tiles] };
        newRow.tiles[tileIndex] = { ...newRow.tiles[tileIndex], char };
        newRows[rowIndex] = newRow;
        return newRows;
      });
      setSolutions(null);
    },
    [],
  );

  const handleTileStateToggle = useCallback((rowIndex: number, tileIndex: number) => {
    setRows((prev) => {
      const newRows = [...prev];
      const newRow = { ...newRows[rowIndex], tiles: [...newRows[rowIndex].tiles] };
      const current = newRow.tiles[tileIndex];
      newRow.tiles[tileIndex] = { ...current, state: cycleState(current.state) };
      newRows[rowIndex] = newRow;
      return newRows;
    });
    setSolutions(null);
  }, []);

  const handleTileSelect = useCallback((rowIndex: number, tileIndex: number) => {
    setSelectedTile({ row: rowIndex, col: tileIndex });
  }, []);

  const handleKeyPress = useCallback(
    (key: string) => {
      if (!selectedTile) return;
      const { row: ri, col: ci } = selectedTile;
      if (key === '⌫') {
        // Backspace: clear current tile, then move to previous
        handleTileCharChange(ri, ci, '');
        if (ci > 0) {
          setSelectedTile({ row: ri, col: ci - 1 });
        }
      } else {
        handleTileCharChange(ri, ci, key);
        // Move to next tile
        if (ci < length - 1) {
          setSelectedTile({ row: ri, col: ci + 1 });
        }
      }
    },
    [selectedTile, length, handleTileCharChange],
  );

  const addRow = useCallback(() => {
    setRows((prev) => [...prev, createBlankRow(length)]);
  }, [length]);

  const removeRow = useCallback(() => {
    setRows((prev) => (prev.length > 1 ? prev.slice(0, -1) : prev));
  }, []);

  const clearAll = useCallback(() => {
    setRows([createBlankRow(length)]);
    setSolutions(null);
    setError(null);
    setSelectedTile(null);
  }, [length]);

  const solveGenerationRef = useRef(0);

  const handleSolve = useCallback(async () => {
    const generation = ++solveGenerationRef.current;
    setLoading(true);
    setError(null);
    setSolutions(null);
    try {
      const res = await solvePuzzle(length, rows, { threads, top: topN });
      if (generation === solveGenerationRef.current) {
        setSolutions(res);
      }
    } catch (e) {
      if (generation === solveGenerationRef.current) {
        const message = e instanceof Error ? e.message : '未知错误';
        setError(`求解失败：${message}`);
      }
    } finally {
      if (generation === solveGenerationRef.current) {
        setLoading(false);
      }
    }
  }, [length, rows, threads, topN]);

  const handleDownload = useCallback(
    (format: DownloadFormat) => {
      if (!solutions) return;
      try {
        downloadResults(solutions, format);
      } catch (e) {
        const message = e instanceof Error ? e.message : '未知错误';
        setError(`下载失败：${message}`);
      }
    },
    [solutions],
  );

  const handleImport = useCallback(
    (importedLength: number, importedRows: GuessRow[]) => {
      // `ImportGameState.parseAndImport` already rejects out-of-range lengths,
      // but clamp again here as defense-in-depth so a stray invalid value
      // can never reach the rendered tile array.
      const clamped = clampLength(importedLength);
      setLength(clamped);
      setLengthDraft(String(clamped));
      setLengthError(null);
      setRows(importedRows.map((row) => adjustRowLength(row, clamped)));
      setSolutions(null);
      setError(null);
      setSelectedTile(null);
    },
    [],
  );

  return (
    <div className={`app${darkMode ? ' dark-theme' : ''}`}>
      <header className="app-header">
        <h1 className="app-title">Sumzle Solver</h1>
        <button
          className="theme-toggle"
          onClick={() => setDarkMode(!darkMode)}
          title={darkMode ? '切换到亮色模式' : '切换到暗色模式'}
        >
          <Icon name={darkMode ? 'sun' : 'moon'} />
        </button>
      </header>

      <main className="app-main">
        <div className="two-column-layout">
          {/* Left Column: Configuration */}
          <div className="column column-left">
            <div className="panel">
              <h2 className="section-title"><Icon name="settings" />求解配置</h2>

              <div className="puzzle-controls">
                <div className="length-control">
                  <label htmlFor="length-input">表达式长度:</label>
                  <input
                    id="length-input"
                    type="number"
                    min={MIN_LENGTH}
                    max={MAX_LENGTH}
                    value={lengthDraft}
                    inputMode="numeric"
                    onChange={(e) => handleLengthDraftChange(e.target.value)}
                    onBlur={handleLengthBlur}
                    className="length-input"
                    aria-invalid={lengthError !== null}
                    aria-describedby={lengthError ? 'length-error' : undefined}
                  />
                </div>
                {lengthError && (
                  <div id="length-error" className="length-error" role="alert">
                    {lengthError}
                  </div>
                )}
                <div className="row-buttons">
                  <button className="btn btn-secondary" onClick={addRow}>
                    + 添加行
                  </button>
                  <button className="btn btn-secondary" onClick={removeRow} disabled={rows.length <= 1}>
                    − 删除行
                  </button>
                  <button className="btn btn-danger" onClick={clearAll}>
                    清空
                  </button>
                </div>
              </div>

              <div className="solve-options">
                <div className="option-control">
                  <label htmlFor="threads-input">线程数:</label>
                  <input
                    id="threads-input"
                    type="number"
                    min={0}
                    max={256}
                    value={threads}
                    onChange={(e) =>
                      setThreads(Math.max(0, Math.min(256, parseInt(e.target.value, 10) || 0)))
                    }
                    className="option-input"
                  />
                  <span className="option-hint">0 表示自动选择</span>
                </div>
                <div className="option-control">
                  <label htmlFor="topn-input">返回最优解数量:</label>
                  <input
                    id="topn-input"
                    type="number"
                    min={0}
                    value={topN}
                    onChange={(e) => setTopN(Math.max(0, Number.parseInt(e.target.value, 10) || 0))}
                    className="option-input"
                  />
                  <span className="option-hint">0 表示返回全部</span>
                </div>
              </div>

              <div className="guess-rows">
                {rows.map((row, i) => (
                  <GuessRowComponent
                    key={i}
                    row={row}
                    rowIndex={i}
                    onTileCharChange={handleTileCharChange}
                    onTileStateToggle={handleTileStateToggle}
                    selectedTile={selectedTile}
                    onTileSelect={handleTileSelect}
                  />
                ))}
              </div>

              <VirtualKeyboard onKeyPress={handleKeyPress} />

              <div className="solve-section">
                <button
                  className="btn btn-primary btn-solve"
                  onClick={handleSolve}
                  disabled={loading}
                >
                  {loading ? '正在求解…' : <><Icon name="play" />开始求解</>}
                </button>
                <p className="solve-hint">
                  先输入猜测，再按反馈标记方块：绿色=正确位置，黄色=存在但错位，灰色=不存在。
                </p>
              </div>

              <ImportGameState length={length} onImport={handleImport} />

              {/* Help section */}
              <div className="help-section">
                <h4 className="help-title"><Icon name="help" />操作提示</h4>
                <ul className="help-list">
                  <li>选中方块后可直接输入，也可使用下方键盘。</li>
                  <li>角标按正确 → 错位 → 不存在循环切换。</li>
                  <li>线程数填 0 让后端自动选择；最优解数量填 0 返回全部解。</li>
                  <li>支持字符：+ - × ÷ % ^ ! A ( ) [ ] = &gt;</li>
                </ul>
              </div>
            </div>
          </div>

          {/* Right Column: Results */}
          <div className="column column-right">
            <div className="panel">
              <Results
                data={solutions}
                loading={loading}
                error={error}
                onDownload={handleDownload}
                onRetry={handleSolve}
              />
            </div>

            {/* Tools section */}
            <div className="panel tools-panel">
              <div className="tools-grid">
                <ExpressionEvaluator />
                <EquationValidator />
              </div>
            </div>
          </div>
        </div>
      </main>

      <footer className="app-footer">
        <p>Sumzle HPC Solver &mdash; Powered by Rust + axum</p>
      </footer>
    </div>
  );
}
