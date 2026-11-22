export interface Store {
  get(key: string): string | undefined;
  put(key: string, value: string): void;
}

export class ReadOnlyStore implements Store {
  get(key: string): string | undefined {
    return undefined;
  }
  put(key: string, value: string): void {
    throw new Error("put is not supported");
  }
}
