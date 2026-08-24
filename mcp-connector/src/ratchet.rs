use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use std::collections::HashMap;
use x25519_dalek::{PublicKey, StaticSecret};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct RatchetState {
    pub dhs: [u8; 32],                // Our current ephemeral DH private key
    pub dhs_pub: [u8; 32],            // Our current ephemeral DH public key
    pub dhr: Option<[u8; 32]>,        // Peer's current ephemeral DH public key
    pub rk: [u8; 32],                 // Root Key
    pub cks: Option<[u8; 32]>,        // Sender Chain Key
    pub ckr: Option<[u8; 32]>,        // Receiver Chain Key
    pub ns: u32,                      // Sender message number
    pub nr: u32,                      // Receiver message number
    pub pn: u32,                      // Previous sender chain length
    pub mkskipped: HashMap<(String, u32), [u8; 32]>, // Skipped message keys: (DHr_hex, nr) -> MK
}

impl RatchetState {
    /// Initialize a new Ratchet for the party that initiates the conversation (Alice).
    pub fn init_alice(shared_secret: [u8; 32], bob_public_key: PublicKey) -> Self {
        let mut dhs_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut dhs_bytes);
        dhs_bytes[0] &= 248; dhs_bytes[31] &= 127; dhs_bytes[31] |= 64;
        let dhs = StaticSecret::from(dhs_bytes);
        let dhs_pub = PublicKey::from(&dhs);

        let mut state = RatchetState {
            dhs: dhs_bytes,
            dhs_pub: dhs_pub.to_bytes(),
            dhr: Some(bob_public_key.to_bytes()),
            rk: shared_secret,
            cks: None,
            ckr: None,
            ns: 0,
            nr: 0,
            pn: 0,
            mkskipped: HashMap::new(),
        };

        // Alice immediately performs the first DH ratchet step to initialize her sending chain
        let dh_out = dhs.diffie_hellman(&bob_public_key);
        let (rk, cks) = Self::kdf_rk(&state.rk, dh_out.as_bytes());
        state.rk = rk;
        state.cks = Some(cks);
        state
    }

    /// Initialize a new Ratchet for the party that receives the initial conversation (Bob).
    pub fn init_bob(shared_secret: [u8; 32], bob_keypair: [u8; 32]) -> Self {
        let secret = StaticSecret::from(bob_keypair);
        let dhs_pub = PublicKey::from(&secret);
        RatchetState {
            dhs: bob_keypair,
            dhs_pub: dhs_pub.to_bytes(),
            dhr: None,
            rk: shared_secret,
            cks: None,
            ckr: None,
            ns: 0,
            nr: 0,
            pn: 0,
            mkskipped: HashMap::new(),
        }
    }

    /// KDF for the Root Chain: Returns (New Root Key, New Chain Key)
    fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
        let hk = Hkdf::<Sha256>::new(Some(rk), dh_out);
        let mut okm = [0u8; 64];
        hk.expand(b"KDF_RK", &mut okm).expect("Crypto operation failed");
        
        let mut new_rk = [0u8; 32];
        let mut new_ck = [0u8; 32];
        new_rk.copy_from_slice(&okm[0..32]);
        new_ck.copy_from_slice(&okm[32..64]);
        
        (new_rk, new_ck)
    }

    /// KDF for the Symmetric Chains: Returns (New Chain Key, Message Key)
    fn kdf_ck(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
        use hmac::{Hmac, Mac};
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(ck).expect("Crypto operation failed");
        mac.update(b"\x01");
        let mk = mac.finalize().into_bytes();

        let mut mac2 = <Hmac<Sha256> as Mac>::new_from_slice(ck).expect("Crypto operation failed");
        mac2.update(b"\x02");
        let new_ck = mac2.finalize().into_bytes();

        let mut out_mk = [0u8; 32];
        out_mk.copy_from_slice(&mk);
        let mut out_ck = [0u8; 32];
        out_ck.copy_from_slice(&new_ck);

        (out_ck, out_mk)
    }

    /// Try to decrypt a skipped message key.
    fn try_skipped_message_keys(&mut self, header_dhr: &[u8; 32], n: u32, ciphertext: &[u8]) -> Option<Vec<u8>> {
        let dhr_hex = hex::encode(header_dhr);
        if let Some(mk) = self.mkskipped.remove(&(dhr_hex.clone(), n)) {
            return Self::decrypt_with_mk(&mk, ciphertext);
        }
        None
    }

    /// Skip message keys if packets arrived out of order.
    fn skip_message_keys(&mut self, until: u32) -> Result<(), &'static str> {
        let dhr_hex = match &self.dhr {
            Some(p) => hex::encode(p),
            None => return Err("No DHR set"),
        };
        
        if self.nr + 1000 < until {
            return Err("Too many skipped messages");
        }
        
        while self.nr < until {
            let ckr = self.ckr.ok_or("No Receiver Chain Key (ckr) initialized")?;
            let (new_ckr, mk) = Self::kdf_ck(&ckr);
            self.ckr = Some(new_ckr);
            self.mkskipped.insert((dhr_hex.clone(), self.nr), mk);
            self.nr += 1;
        }
        Ok(())
    }

    /// Perform a DH Ratchet step.
    fn dh_ratchet(&mut self, header_dhr: [u8; 32]) {
        self.pn = self.ns;
        self.ns = 0;
        self.nr = 0;
        self.dhr = Some(header_dhr);
        
        let dhr_pub = PublicKey::from(header_dhr);
        let my_dhs = StaticSecret::from(self.dhs);
        
        // 1. Step Root KDF to get Receiver Chain Key using OUR old private key + THEIR new public key
        let dh_recv = my_dhs.diffie_hellman(&dhr_pub);
        let (rk1, ckr) = Self::kdf_rk(&self.rk, dh_recv.as_bytes());
        self.rk = rk1;
        self.ckr = Some(ckr);

        // 2. Generate new Ephemeral KeyPair for OUR next sending chain
        let mut dhs_bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut dhs_bytes);
        dhs_bytes[0] &= 248; dhs_bytes[31] &= 127; dhs_bytes[31] |= 64;
        self.dhs = dhs_bytes;
        
        let new_dhs = StaticSecret::from(dhs_bytes);
        self.dhs_pub = PublicKey::from(&new_dhs).to_bytes();

        // 3. Step Root KDF to get Sender Chain Key using OUR new private key + THEIR new public key
        let dh_send = new_dhs.diffie_hellman(&dhr_pub);
        let (rk2, cks) = Self::kdf_rk(&self.rk, dh_send.as_bytes());
        self.rk = rk2;
        self.cks = Some(cks);
    }

    /// Encrypt a payload.
    pub fn ratchet_encrypt(&mut self, plaintext: &[u8]) -> Result<([u8; 32], u32, u32, Vec<u8>), &'static str> {
        let cks = self.cks.ok_or("No Sender Chain Key (cks) initialized")?;
        let (new_cks, mk) = Self::kdf_ck(&cks);
        self.cks = Some(new_cks);
        
        let mut iv = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut iv);
        let cipher = Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(&mk));
        let nonce_gcm = Nonce::from_slice(&iv);
        
        let ciphertext_only = cipher.encrypt(nonce_gcm, plaintext).map_err(|_| "Encryption failed")?;
        
        // Final payload structure: IV (12 bytes) || Ciphertext
        let mut final_payload = iv.to_vec();
        final_payload.extend(ciphertext_only);
        
        let header_pub = self.dhs_pub.clone();
        let header_n = self.ns;
        let header_pn = self.pn;
        
        self.ns += 1;
        
        Ok((header_pub, header_n, header_pn, final_payload))
    }

    /// Decrypt a payload.
    pub fn ratchet_decrypt(&mut self, header_dhr: [u8; 32], header_n: u32, header_pn: u32, ciphertext_with_iv: &[u8]) -> Result<Vec<u8>, &'static str> {
        // 1. Check if this is a delayed message we already skipped over
        if let Some(plaintext) = self.try_skipped_message_keys(&header_dhr, header_n, ciphertext_with_iv) {
            return Ok(plaintext);
        }

        // 2. If the peer's public key changed, perform a DH Ratchet step
        if Some(header_dhr) != self.dhr {
            self.skip_message_keys(header_pn)?;
            self.dh_ratchet(header_dhr);
        }

        // 3. Skip message keys in the current receiving chain if needed
        self.skip_message_keys(header_n)?;

        // 4. Step the receiving chain for this message
        let ckr = self.ckr.ok_or("No CKR set")?;
        let (new_ckr, mk) = Self::kdf_ck(&ckr);
        self.ckr = Some(new_ckr);
        self.nr += 1;

        // 5. Decrypt
        match Self::decrypt_with_mk(&mk, ciphertext_with_iv) {
            Some(pt) => Ok(pt),
            None => Err("Decryption failed")
        }
    }

    fn decrypt_with_mk(mk: &[u8; 32], ciphertext_with_iv: &[u8]) -> Option<Vec<u8>> {
        if ciphertext_with_iv.len() < 12 { return None; }
        let (iv, ciphertext) = ciphertext_with_iv.split_at(12);
        
        let cipher = Aes256Gcm::new(aes_gcm::Key::<Aes256Gcm>::from_slice(mk));
        let nonce_gcm = Nonce::from_slice(iv);
        
        cipher.decrypt(nonce_gcm, ciphertext).ok()
    }
}
