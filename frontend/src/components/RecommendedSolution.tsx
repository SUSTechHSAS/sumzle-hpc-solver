import { displayChar } from '../types';
import './RecommendedSolution.css';

interface RecommendedSolutionProps {
  solution: string | null;
}

export default function RecommendedSolution({ solution }: RecommendedSolutionProps) {
  if (!solution) {
    return null;
  }

  return (
    <div className="recommended-section">
      <h3 className="section-title">⭐ 推荐解</h3>
      <div className="recommended-result-item">
        <span className="recommended-label">⭐推荐</span>
        <span className="recommended-expression">
          {solution.split('').map(displayChar).join('')}
        </span>
      </div>
    </div>
  );
}
