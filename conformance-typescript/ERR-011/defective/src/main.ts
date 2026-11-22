export function port(raw: string): number {
  return parseInt(raw, 10) || 8080;
}

export function ratio(raw: string): number {
  return Number(raw) || 1;
}
