// SPDX-License-Identifier: MIT OR Apache-2.0
export function port(raw: string): number {
  const n = parseInt(raw, 10);
  if (Number.isNaN(n)) {
    throw new Error(`not a port: ${raw}`);
  }
  return n;
}

export function ratio(raw: string): number {
  // lenient on purpose: an unreadable ratio means no scaling
  return Number(raw) || 1;
}

export function count(items: number[]): number {
  return items.length || 0;
}
