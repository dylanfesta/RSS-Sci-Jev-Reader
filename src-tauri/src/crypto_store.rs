//! Password-based AES-256-GCM encryption store for AI Provider API keys.
//! Binary/JSON compatible with the Python `CryptoStore` implementation (`secrets.enc`),
//! while also supporting multi-provider encrypted API key storage.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub enum CryptoError {
    ValueError(String),
    NotFound(String),
    DecryptionError(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::ValueError(s) => write!(f, "{s}"),
            CryptoError::NotFound(s) => write!(f, "{s}"),
            CryptoError::DecryptionError(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct EncryptedPayload {
    version: u32,
    kdf: String,
    iterations: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_ciphertext: Option<String>,
}

#[derive(Clone)]
pub struct CryptoStore {
    secret_path: Arc<RwLock<PathBuf>>,
    unlocked_api_key: Arc<RwLock<Option<String>>>,
    unlocked_provider_keys: Arc<RwLock<HashMap<String, String>>>,
}

impl CryptoStore {
    pub const KDF_ITERATIONS: u32 = 600_000;
    pub const SALT_BYTES: usize = 16;
    pub const NONCE_BYTES: usize = 12;

    pub fn new(secret_path: impl AsRef<Path>) -> Self {
        Self {
            secret_path: Arc::new(RwLock::new(secret_path.as_ref().to_path_buf())),
            unlocked_api_key: Arc::new(RwLock::new(None)),
            unlocked_provider_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_secret_path(&self, new_path: impl AsRef<Path>) {
        if let Ok(mut guard) = self.secret_path.write() {
            *guard = new_path.as_ref().to_path_buf();
        }
    }

    pub fn get_secret_path(&self) -> PathBuf {
        self.secret_path.read().unwrap().clone()
    }

    fn derive_key(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
        let mut derived = [0u8; 32];
        pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut derived);
        derived
    }

    pub fn has_saved_key(&self) -> bool {
        self.get_secret_path().is_file()
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked_api_key
            .read()
            .map(|g| g.as_ref().map(|s| !s.is_empty()).unwrap_or(false))
            .unwrap_or(false)
    }

    pub fn get_api_key(&self) -> Option<String> {
        self.unlocked_api_key.read().ok().and_then(|g| g.clone())
    }

    pub fn get_api_key_for_provider(&self, provider_id: &str) -> Option<String> {
        let clean_id = provider_id.trim().to_lowercase();
        if let Ok(map) = self.unlocked_provider_keys.read() {
            if let Some(k) = map.get(&clean_id) {
                if !k.is_empty() {
                    return Some(k.clone());
                }
            }
        }
        self.get_api_key()
    }

    pub fn mask_key_str(key: &str) -> Option<String> {
        if key.is_empty() {
            return None;
        }
        let chars: Vec<char> = key.chars().collect();
        if chars.len() <= 10 {
            return Some("***".to_string());
        }
        let prefix: String = chars[..7].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        Some(format!("{prefix}...{suffix}"))
    }

    pub fn get_masked_key(&self) -> Option<String> {
        let key = self.get_api_key()?;
        Self::mask_key_str(&key)
    }

    pub fn get_masked_key_for_provider(&self, provider_id: &str) -> Option<String> {
        let key = self.get_api_key_for_provider(provider_id)?;
        Self::mask_key_str(&key)
    }

    pub fn list_unlocked_providers(&self) -> Vec<String> {
        self.unlocked_provider_keys
            .read()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn save_api_key(&self, api_key: &str, password: &str) -> Result<(), CryptoError> {
        self.save_api_key_for_provider(None, api_key, password)
    }

    pub fn save_api_key_for_provider(
        &self,
        provider_id: Option<&str>,
        api_key: &str,
        password: &str,
    ) -> Result<(), CryptoError> {
        let clean_key = api_key.trim();
        if clean_key.is_empty() {
            return Err(CryptoError::ValueError(
                "API key cannot be empty.".to_string(),
            ));
        }
        if password.is_empty() {
            return Err(CryptoError::ValueError(
                "Password cannot be empty.".to_string(),
            ));
        }

        let mut provider_map = self
            .unlocked_provider_keys
            .read()
            .map(|m| m.clone())
            .unwrap_or_default();
        let pid = provider_id
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "openrouter".to_string());
        provider_map.insert(pid, clean_key.to_string());

        let mut salt = [0u8; Self::SALT_BYTES];
        let mut nonce_bytes = [0u8; Self::NONCE_BYTES];
        let mut prov_nonce_bytes = [0u8; Self::NONCE_BYTES];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        rand::thread_rng().fill_bytes(&mut prov_nonce_bytes);

        let derived_key = Self::derive_key(password, &salt, Self::KDF_ITERATIONS);
        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| CryptoError::ValueError(e.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, clean_key.as_bytes())
            .map_err(|e| CryptoError::ValueError(format!("Encryption failed: {e}")))?;

        let map_json = serde_json::to_string(&provider_map).unwrap_or_else(|_| "{}".to_string());
        let prov_nonce = Nonce::from_slice(&prov_nonce_bytes);
        let prov_ciphertext = cipher
            .encrypt(prov_nonce, map_json.as_bytes())
            .map_err(|e| CryptoError::ValueError(format!("Encryption failed: {e}")))?;

        let payload = EncryptedPayload {
            version: 1,
            kdf: "PBKDF2-HMAC-SHA256".to_string(),
            iterations: Self::KDF_ITERATIONS,
            salt: BASE64.encode(salt),
            nonce: BASE64.encode(nonce_bytes),
            ciphertext: BASE64.encode(ciphertext),
            provider_nonce: Some(BASE64.encode(prov_nonce_bytes)),
            provider_ciphertext: Some(BASE64.encode(prov_ciphertext)),
        };

        let secret_path = self.get_secret_path();
        if let Some(parent) = secret_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let tmp_path = secret_path.with_extension("tmp");
        let json_str = serde_json::to_string_pretty(&payload)
            .map_err(|e| CryptoError::ValueError(e.to_string()))?;
        fs::write(&tmp_path, json_str.as_bytes())
            .map_err(|e| CryptoError::ValueError(format!("Failed to write key file: {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        }

        fs::rename(&tmp_path, &secret_path)
            .map_err(|e| CryptoError::ValueError(format!("Failed to finalize key file: {e}")))?;

        if let Ok(mut guard) = self.unlocked_api_key.write() {
            *guard = Some(clean_key.to_string());
        }
        if let Ok(mut mguard) = self.unlocked_provider_keys.write() {
            *mguard = provider_map;
        }
        Ok(())
    }

    pub fn unlock_api_key(&self, password: &str) -> Result<String, CryptoError> {
        if !self.has_saved_key() {
            return Err(CryptoError::NotFound(
                "No encrypted API key file found.".to_string(),
            ));
        }
        if password.is_empty() {
            return Err(CryptoError::DecryptionError(
                "Password is required.".to_string(),
            ));
        }

        let secret_path = self.get_secret_path();
        let raw_text = fs::read_to_string(&secret_path).map_err(|_| {
            CryptoError::DecryptionError("Corrupted encrypted key file.".to_string())
        })?;
        let payload: EncryptedPayload = serde_json::from_str(&raw_text).map_err(|_| {
            CryptoError::DecryptionError("Corrupted encrypted key file.".to_string())
        })?;

        let salt = BASE64.decode(&payload.salt).map_err(|_| {
            CryptoError::DecryptionError("Corrupted encrypted key file.".to_string())
        })?;
        let nonce_bytes = BASE64.decode(&payload.nonce).map_err(|_| {
            CryptoError::DecryptionError("Corrupted encrypted key file.".to_string())
        })?;
        let ciphertext = BASE64.decode(&payload.ciphertext).map_err(|_| {
            CryptoError::DecryptionError("Corrupted encrypted key file.".to_string())
        })?;
        if nonce_bytes.len() != Self::NONCE_BYTES {
            return Err(CryptoError::DecryptionError(
                "Corrupted encrypted key file.".to_string(),
            ));
        }

        let iterations = if payload.iterations > 0 {
            payload.iterations
        } else {
            Self::KDF_ITERATIONS
        };
        let derived_key = Self::derive_key(password, &salt, iterations);
        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|_| CryptoError::DecryptionError("Incorrect password.".to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| CryptoError::DecryptionError("Incorrect password.".to_string()))?;

        let decoded = String::from_utf8(plaintext).map_err(|_| {
            CryptoError::DecryptionError("Corrupted encrypted key file.".to_string())
        })?;

        let mut provider_map = HashMap::new();
        if let (Some(p_nonce_b64), Some(p_ct_b64)) =
            (&payload.provider_nonce, &payload.provider_ciphertext)
        {
            if let (Ok(p_nonce_bytes), Ok(p_ct)) =
                (BASE64.decode(p_nonce_b64), BASE64.decode(p_ct_b64))
            {
                if p_nonce_bytes.len() == Self::NONCE_BYTES {
                    let p_nonce = Nonce::from_slice(&p_nonce_bytes);
                    if let Ok(p_plain) = cipher.decrypt(p_nonce, p_ct.as_ref()) {
                        if let Ok(m) = serde_json::from_slice::<HashMap<String, String>>(&p_plain) {
                            provider_map = m;
                        }
                    }
                }
            }
        }
        if provider_map.is_empty() {
            provider_map.insert("openrouter".to_string(), decoded.clone());
        }

        if let Ok(mut guard) = self.unlocked_api_key.write() {
            *guard = Some(decoded.clone());
        }
        if let Ok(mut mguard) = self.unlocked_provider_keys.write() {
            *mguard = provider_map;
        }
        Ok(decoded)
    }

    pub fn lock(&self) {
        if let Ok(mut guard) = self.unlocked_api_key.write() {
            *guard = None;
        }
        if let Ok(mut mguard) = self.unlocked_provider_keys.write() {
            mguard.clear();
        }
    }

    pub fn delete_saved_key(&self) {
        self.lock();
        let secret_path = self.get_secret_path();
        if secret_path.exists() {
            let _ = fs::remove_file(secret_path);
        }
    }
}
