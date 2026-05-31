import type {
  SolveRequest,
  SolveResponse,
  ValidateRequest,
  ValidateResponse,
  EvalRequest,
  EvalResponse,
  DownloadFormat,
} from './types';

const API_BASE = '/api';

export async function solvePuzzle(
  length: number,
  rows: SolveRequest['rows'],
  threads = 0,
): Promise<SolveResponse> {
  const res = await fetch(`${API_BASE}/solve?threads=${threads}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ length, rows }),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function downloadResults(
  length: number,
  rows: SolveRequest['rows'],
  format: DownloadFormat = 'json',
): Promise<void> {
  const res = await fetch(`${API_BASE}/download?format=${format}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ length, rows }),
  });
  if (!res.ok) throw new Error(await res.text());

  const blob = await res.blob();
  const contentDisposition = res.headers.get('content-disposition');
  let filename = `sumzle_solutions.${format}`;
  if (contentDisposition) {
    const match = contentDisposition.match(/filename="?(.+?)"?$/);
    if (match) filename = match[1];
  }

  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export async function validateEquation(equation: string): Promise<ValidateResponse> {
  const body: ValidateRequest = { equation };
  const res = await fetch(`${API_BASE}/validate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function evaluateExpression(expression: string): Promise<EvalResponse> {
  const body: EvalRequest = { expression };
  const res = await fetch(`${API_BASE}/eval`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}
