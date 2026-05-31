import type { SolveResponse, DownloadFormat } from '../types';
import { displayChar } from '../types';
import { formatSpeed, formatTime } from '../utils';
import CharacterProbability from './CharacterProbability';
import RecommendedSolution from './RecommendedSolution';
import './Results.css';

interface ResultsProps {
  data: SolveResponse | null;
  loading: boolean;
  error: string | null;
  onDownload: (format: DownloadFormat) => void;
}

const MAX_DISPLAY_SOLUTIONS = 500;

export default function Results({ data, loading, error, onDownload }: ResultsProps) {
  if (loading) {
    return (
      <div className="results-section">
        <div className="results-loading">
          <div className="spinner" />
          <span>正在求解...</span>
        </div>
        <div className="progress-bar-container">
          <div className="progress-bar animated" />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="results-section">
        <div className="results-error">
          <span className="error-icon">⚠️</span>
          <span>{error}</span>
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="results-section">
        <div className="results-placeholder">等待求解开始...</div>
      </div>
    );
  }

  const { solutions, stats, char_probabilities, recommended } = data;
  const truncated = solutions.length > MAX_DISPLAY_SOLUTIONS;

  return (
    <div className="results-section" data-testid="results-section">
      <h2 className="section-title">📊 求解结果</h2>

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
          <h3 className="section-title">📜 结果列表</h3>
          <span className="results-count">
            找到 <strong>{stats.found_count.toLocaleString()}</strong> 个解
            {truncated && <span className="truncation-notice"> (仅显示前{MAX_DISPLAY_SOLUTIONS})</span>}
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
                {recommended === sol && <span className="result-recommended-badge">⭐推荐</span>}
              </div>
            ))}
          </div>
        ) : (
          <div className="no-solutions">暂无找到符合条件的解</div>
        )}
      </div>

      {/* Download buttons */}
      {solutions.length > 0 && (
        <div className="download-section">
          <h3 className="section-title">📥 下载结果</h3>
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
