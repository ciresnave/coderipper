const table = new Map<string, number>();

export function lookup(key: string): number {
  return table.get(key)!;
}

export function first(items: string[]): string {
  const head = items[0]!;
  return head;
}
