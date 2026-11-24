// SPDX-License-Identifier: MIT OR Apache-2.0
const table = new Map<string, number>();

export function lookup(key: string): number {
  const found = table.get(key);
  if (found === undefined) {
    throw new Error(`missing ${key}`);
  }
  return found !== 0 ? found : 0;
}

export function first(items: string[]): string {
  // INVARIANT: callers pass a non-empty list (checked at the boundary)
  return items[0]!;
}

export class Holder {
  value!: number;
}
