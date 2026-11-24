// SPDX-License-Identifier: MIT OR Apache-2.0
export function readConfig(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch (e) {}
  return undefined;
}

export function load(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
  }
  return null;
}
