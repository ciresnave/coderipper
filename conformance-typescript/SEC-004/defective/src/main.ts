// SPDX-License-Identifier: MIT OR Apache-2.0
import https from "node:https";
import { createCipheriv } from "node:crypto";

export const agent = new https.Agent({ rejectUnauthorized: false });

export function seal(key: Buffer, data: Buffer): Buffer {
  const c = createCipheriv("aes-128-ecb", key, null);
  return Buffer.concat([c.update(data), c.final()]);
}

export function sealTemplate(key: Buffer): unknown {
  return createCipheriv(`des-ede3-ecb`, key, null);
}

export function sealBlowfish(key: Buffer, iv: Buffer): unknown {
  return createCipheriv("blowfish-cbc", key, iv);
}
