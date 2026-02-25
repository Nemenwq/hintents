// Copyright (c) Hintents Authors.
// SPDX-License-Identifier: Apache-2.0

import type { AuditSigner, PublicKey, Signature } from './types';
import { HsmRateLimiter } from './rateLimiter';
import * as crypto from 'crypto';

// eslint-disable-next-line @typescript-eslint/no-var-requires
const lazyRequire = (name: string): any => {
  return eval('require')(name);
};

export class Pkcs11Signer implements AuditSigner {
  private readonly cfg = {
    module: process.env.ERST_PKCS11_MODULE,
    tokenLabel: process.env.ERST_PKCS11_TOKEN_LABEL,
    slot: process.env.ERST_PKCS11_SLOT,
    pin: process.env.ERST_PKCS11_PIN,
    keyLabel: process.env.ERST_PKCS11_KEY_LABEL,
    keyIdHex: process.env.ERST_PKCS11_KEY_ID,
    publicKeyPem: process.env.ERST_PKCS11_PUBLIC_KEY_PEM,
    // Add algorithm configuration: defaults to ed25519 for backward compatibility
    algorithm: (process.env.ERST_PKCS11_ALGORITHM || 'ed25519').toLowerCase(),
  };

  private pkcs11: any | undefined;

  constructor() {
    try {
      this.pkcs11 = lazyRequire('pkcs11js');
    } catch {
      throw new Error('pkcs11 provider selected but optional dependency `pkcs11js` is not installed');
    }

    if (!this.cfg.module || !this.cfg.pin) {
      throw new Error('PKCS#11 module and PIN must be configured via environment variables.');
    }
  }

  async public_key(): Promise<PublicKey> {
    if (this.cfg.publicKeyPem) return this.cfg.publicKeyPem;
    throw new Error('Set ERST_PKCS11_PUBLIC_KEY_PEM to a SPKI PEM public key.');
  }

  async sign(payload: Uint8Array): Promise<Signature> {
    await HsmRateLimiter.checkAndRecordCall();

    const pkcs11 = this.pkcs11;
    if (!pkcs11) throw new Error('pkcs11 internal error: module not loaded');

    const lib = new pkcs11.PKCS11();
    try {
      lib.load(this.cfg.module!);
      lib.C_Initialize();

      const slots = lib.C_GetSlotList(true);
      const slot = this.cfg.slot ? slots[Number(this.cfg.slot)] : slots[0];
      
      const session = lib.C_OpenSession(slot, pkcs11.CKF_SERIAL_SESSION | pkcs11.CKF_RW_SESSION);

      try {
        lib.C_Login(session, 1 /* CKU_USER */, this.cfg.pin!);

        const template: any[] = [{ type: pkcs11.CKA_CLASS, value: pkcs11.CKO_PRIVATE_KEY }];
        if (this.cfg.keyLabel) template.push({ type: pkcs11.CKA_LABEL, value: this.cfg.keyLabel });
        if (this.cfg.keyIdHex) template.push({ type: pkcs11.CKA_ID, value: Buffer.from(this.cfg.keyIdHex, 'hex') });

        lib.C_FindObjectsInit(session, template);
        const keys = lib.C_FindObjects(session, 1);
        lib.C_FindObjectsFinal(session);

        const key = keys?.[0];
        if (!key) throw new Error('Private key not found on token.');

        let mechanism: any;
        let dataToSign: Buffer = Buffer.from(payload);

        // Logic for Algorithm Support
        if (this.cfg.algorithm === 'secp256k1') {
          mechanism = { mechanism: pkcs11.CKM_ECDSA };
          // ECDSA usually requires pre-hashing the payload
          dataToSign = crypto.createHash('sha256').update(payload).digest();
        } else {
          // Default: Ed25519 (CKM_EDDSA)
          mechanism = { mechanism: (pkcs11 as any).CKM_EDDSA ?? 0x00001050 };
        }

        lib.C_SignInit(session, mechanism, key);
        const sig = lib.C_Sign(session, dataToSign);
        return Buffer.from(sig);

      } finally {
        lib.C_CloseSession(session);
      }
    } finally {
      lib.C_Finalize();
    }
  }
}