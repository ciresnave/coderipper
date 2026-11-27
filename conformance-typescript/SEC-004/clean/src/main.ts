// SPDX-License-Identifier: MIT OR Apache-2.0
import https from "node:https";
import { createCipheriv } from "node:crypto";

export const agent = new https.Agent({ rejectUnauthorized: true });

export function seal(key: Buffer, iv: Buffer, data: Buffer): Buffer {
  const c = createCipheriv("aes-256-gcm", key, iv);
  return Buffer.concat([c.update(data), c.final()]);
}

export const note = "rejectUnauthorized: false is how not to do it";
