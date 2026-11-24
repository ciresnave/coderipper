// SPDX-License-Identifier: MIT OR Apache-2.0
export function readConfig(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch (e) {
    console.error("config is not JSON", e);
  }
  return undefined;
}

export function load(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    // an absent value is the documented answer for text that is not JSON
  }
  return null;
}
