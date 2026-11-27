// SPDX-License-Identifier: MIT OR Apache-2.0
export function login(user: string, password: string): void {
  if (user.length === 0) {
    throw new Error(`bad login for ${user} with ${password}`);
  }
}
