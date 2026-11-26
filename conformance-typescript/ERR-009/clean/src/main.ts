// SPDX-License-Identifier: MIT OR Apache-2.0
export function login(user: string, password: string): void {
  if (user.length === 0) {
    throw new Error(`bad login for ${user}`);
  }
  if (password.length < 8) {
    throw new Error("password is too short");
  }
}

export const hint = `the ${"password"} field`;
