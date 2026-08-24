import Olm from '@matrix-org/olm';

export const initOlm = async () => {
  await Olm.init();
};

export const generateOlmAccount = () => {
  let account = new Olm.Account();
  account.create();
  return account;
};

export const generateOneTimeKeys = (account: any, numberOfKeys: number = 50) => {
  account.generate_one_time_keys(numberOfKeys);
  return account.one_time_keys();
};

export const signData = (account: any, message: string) => {
  return account.sign(message);
};

export const encryptBackupKey = async (password: string, keyToBackup: Uint8Array): Promise<string> => {
  const enc = new TextEncoder();
  const passwordKey = await window.crypto.subtle.importKey(
    'raw',
    enc.encode(password),
    { name: 'PBKDF2' },
    false,
    ['deriveBits', 'deriveKey']
  );

  const salt = window.crypto.getRandomValues(new Uint8Array(16));
  const iv = window.crypto.getRandomValues(new Uint8Array(12));

  const secretKey = await window.crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt,
      iterations: 600000,
      hash: 'SHA-256'
    },
    passwordKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt']
  );

  const ciphertextBuffer = await window.crypto.subtle.encrypt(
    {
      name: 'AES-GCM',
      iv,
      tagLength: 128
    },
    secretKey,
    keyToBackup
  );

  const ciphertext = new Uint8Array(ciphertextBuffer);

  // Helper to convert Uint8Array to Base64
  const toBase64 = (arr: Uint8Array) => btoa(String.fromCharCode(...arr));

  return `${toBase64(salt)}:${toBase64(iv)}:${toBase64(ciphertext)}`;
};

export const decryptBackupKey = async (password: string, backupString: string): Promise<Uint8Array> => {
  const [saltB64, ivB64, ciphertextB64] = backupString.split(':');
  
  const fromBase64 = (b64: string) => Uint8Array.from(atob(b64), c => c.charCodeAt(0));
  
  const salt = fromBase64(saltB64);
  const iv = fromBase64(ivB64);
  const ciphertext = fromBase64(ciphertextB64);

  const enc = new TextEncoder();
  const passwordKey = await window.crypto.subtle.importKey(
    'raw',
    enc.encode(password),
    { name: 'PBKDF2' },
    false,
    ['deriveBits', 'deriveKey']
  );

  const secretKey = await window.crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt,
      iterations: 600000,
      hash: 'SHA-256'
    },
    passwordKey,
    { name: 'AES-GCM', length: 256 },
    false,
    ['decrypt']
  );

  const decryptedBuffer = await window.crypto.subtle.decrypt(
    {
      name: 'AES-GCM',
      iv,
      tagLength: 128
    },
    secretKey,
    ciphertext
  );

  return new Uint8Array(decryptedBuffer);
};

export const generateSecurityNumber = async (pubKeyA: string, pubKeyB: string): Promise<string> => {
  const sorted = [pubKeyA, pubKeyB].sort();
  const concatenated = sorted.join('');
  const enc = new TextEncoder();
  const data = enc.encode(concatenated);
  
  const hashBuffer = await window.crypto.subtle.digest('SHA-512', data);
  const hashArray = new Uint8Array(hashBuffer);
  
  let result = [];
  for (let i = 0; i < 12; i++) {
    let num = 0;
    for (let j = 0; j < 5; j++) {
      num = (num * 256) + hashArray[i * 5 + j];
    }
    result.push((num % 100000).toString().padStart(5, '0'));
  }
  return result.join(' ');
};
