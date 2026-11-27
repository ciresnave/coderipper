// SPDX-License-Identifier: MIT OR Apache-2.0
export function near(x: number): boolean {
  return Math.abs(x - 0.1) < 1e-9;
}

export function isZero(x: number): boolean {
  return x === 0;
}

export function whole(x: number): boolean {
  return x === 1.0 || x === 100;
}

export function label(s: string): boolean {
  return s === "0.1";
}

export function ordered(x: number): boolean {
  return x < 0.5 && x >= 0.25;
}
