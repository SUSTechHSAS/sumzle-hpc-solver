import type {
  SolveRequest,
  SolveResponse,
  ValidateRequest,
  ValidateResponse,
  EvalRequest,
  EvalResponse,
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
