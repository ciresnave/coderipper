export interface Store {
  get(key: string): string | undefined;
  put(key: string, value: string): void;
}

export class MemoryStore implements Store {
  private data = new Map<string, string>();
  get(key: string): string | undefined {
    return this.data.get(key);
  }
  put(key: string, value: string): void {
    if (key === "") {
      throw new Error("empty key");
    }
    this.data.set(key, value);
  }
}

export class Plain {
  later(): void {
    throw new Error("not implemented");
  }
}
