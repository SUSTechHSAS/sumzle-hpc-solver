import type { SolveResponse, DownloadFormat, SolveProgress } from '../types';
import { displayChar, progressPhaseLabel } from '../types';
import { formatSpeed, formatTime } from '../utils';
import CharacterProbability from './CharacterProbability';
import RecommendedSolution from './RecommendedSolution';
import Icon from './Icon';
import './Results.css';

interface ResultsProps {
  data: SolveResponse | null;
  loading: boolean;
  error: string | null;
  onDownload: (format: DownloadFormat) => void;
  onRetry?: () => void;
  /** Live solve progress (issue #22); null for a plain non-streaming solve. */
  progress?: SolveProgress | null;
}

const MAX_DISPLAY_SOLUTIONS = 500;

export default function Results({ data, loading, error, onDownload, onRetry, progress }: ResultsProps) {
  if (loading) {
    // Determinate bar when we have branch-completion progress; otherwise the
    // original indeterminate pulse (plain solve, or before the first event).
    const live = progress && progress.total > 0 ? progress : null;
    const pct = live ? Math.min(100, Math.round((live.done / live.total) * 100)) : null;
    return (
      <div className="results-section">
        <div className="results-loading" role="status" aria-live="polite">
          <div className="spinner" />
          <span>
            {live ? `${progressPhaseLabel(live.phase)}… ${pct}%` : '正在搜索候选表达式…'}
          </span>
        </div>
        <div className="progress-bar-container">
          {live ? (
            <div
              className="progress-bar determinate"
              style={{ transform: `scaleX(${(pct ?? 0) / 100})` }}
            />
          ) : (
            <div className="progress-bar animated" />
          )}
        </div>
        {live && (
          <div className="progress-detail">
            {live.done.toLocaleString()} / {live.total.toLocaleString()} 分支
          </div>
        )}
      </div>
    );
  }

  if (error) {
    return (
      <div className="results-section">
        <div className="results-error" role="alert">
          <span className="error-icon"><Icon name="alert" /></span>
          <span className="results-error-message">{error}</span>
          {onRetry && (
            <button className="results-retry" type="button" onClick={onRetry}>
              重新求解
            </button>
          )}
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="results-section">
        <div className="results-placeholder">输入反馈后运行求解，结果会显示在这里。</div>
      </div>
    );
  }

  const { solutions, stats, char_probabilities, recommended } = data;
  // Tolerate responses from an older backend that omit these fields.
  const top = data.top ?? 0;
  const scores = data.scores ?? [];
  const truncated = solutions.length > MAX_DISPLAY_SOLUTIONS;
  const hasScores = scores.length === solutions.length && scores.length > 0;

  return (
    <div className="results-section" data-testid="results-section">
      <h2 className="section-title"><Icon name="chart" />求解结果</h2>

      {/* Stats cards */}
      <div className="stats-grid">
        <div className="stat-card">
          <span className="stat-value">{stats.found_count.toLocaleString()}</span>
          <span className="stat-label">找到解数</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{stats.searched_count.toLocaleString()}</span>
          <span className="stat-label">搜索表达式</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{formatTime(stats.elapsed_ms)}</span>
          <span className="stat-label">用时</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{formatSpeed(stats.speed)}</span>
          <span className="stat-label">速度 (expr/s)</span>
        </div>
      </div>

      {/* Recommended solution */}
      <RecommendedSolution solution={recommended} />

      {/* Character probabilities */}
      <CharacterProbability probabilities={char_probabilities} />

      {/* Results list */}
      <div className="results-list-section">
        <div className="results-list-header">
          <h3 className="section-title"><Icon name="table" />结果列表</h3>
          <span className="results-count">
            {top > 0 ? (
              <>
                显示最优 <strong>{solutions.length.toLocaleString()}</strong> 个解
                <span className="top-n-badge">Top-{top}</span>
              </>
            ) : (
              <>
                找到 <strong>{stats.found_count.toLocaleString()}</strong> 个解
                {truncated && (
                  <span className="truncation-notice"> (仅显示前{MAX_DISPLAY_SOLUTIONS})</span>
                )}
              </>
            )}
          </span>
        </div>
        {solutions.length > 0 ? (
          <div className="results-container">
            {(truncated ? solutions.slice(0, MAX_DISPLAY_SOLUTIONS) : solutions).map((sol, i) => (
              <div
                key={i}
                className={`result-item${recommended === sol ? ' recommended' : ''}`}
              >
                <span className="result-index">{i + 1}.</span>
                <span className="result-expression">
                  {sol.split('').map(displayChar).join('')}
                </span>
                {hasScores && <span className="result-score">{scores[i].toFixed(1)}</span>}
                {recommended === sol && <span className="result-recommended-badge"><Icon name="star" />推荐</span>}
              </div>
            ))}
          </div>
        ) : (
          <div className="no-solutions">没有符合当前反馈的解。请检查颜色状态或表达式长度。</div>
        )}
      </div>

      {/* Download buttons */}
      {solutions.length > 0 && (
        <div className="download-section">
          <h3 className="section-title"><Icon name="download" />下载结果</h3>
          <div className="download-buttons">
            <button
              className="btn btn-download"
              onClick={() => onDownload('json')}
            >
              JSON
            </button>
            <button
              className="btn btn-download"
              onClick={() => onDownload('csv')}
            >
              CSV
            </button>
            <button
              className="btn btn-download"
              onClick={() => onDownload('txt')}
            >
              TXT
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
