//! ED25519 key management for OwnMon.
//!
//! - Generates keypair on first run
//! - Stores private key in Windows Credential Manager
//! - Stores public key in config directory

use base64::Engine;
use ed25519_dalek::{SigningKey, VerifyingKey, SECRET_KEY_LENGTH};
use rand::rngs::OsRng;
use std::path::PathBuf;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS,
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

const CREDENTIAL_TARGET: &str = "OwnMon_ED25519_PrivateKey";

/// Key manager for ED25519 signing operations.
pub struct KeyManager {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl KeyManager {
    /// Initialize key manager - loads existing keys or generates new ones.
    pub fn init() -> Result<Self, KeyError> {
        match Self::load_private_key() {
            Ok(signing_key) => {
                let verifying_key = signing_key.verifying_key();
                tracing::info!("Loaded existing ED25519 keypair");
                Ok(Self {
                    signing_key,
                    verifying_key,
                })
            }
            Err(KeyError::NotFound) => {
                tracing::info!("No existing keypair found, generating new one");
                Self::generate_new()
            }
            Err(e) => Err(e),
        }
    }

    /// Generate new keypair and store it.
    fn generate_new() -> Result<Self, KeyError> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Store private key in Credential Manager
        Self::store_private_key(&signing_key)?;

        // Store public key to file
        Self::store_public_key(&verifying_key)?;

        tracing::info!("Generated and stored new ED25519 keypair");
        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Get reference to signing key for signing operations.
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Get reference to verifying key for verification.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Get public key as base64 string.
    pub fn public_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.verifying_key.as_bytes())
    }

    /// Get public key file path.
    pub fn public_key_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ownmon")
            .join("public_key.txt")
    }

    /// Load private key from Windows Credential Manager.
    fn load_private_key() -> Result<SigningKey, KeyError> {
        unsafe {
            let target: Vec<u16> = CREDENTIAL_TARGET
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut credential_ptr: *mut CREDENTIALW = std::ptr::null_mut();

            let result = CredReadW(
                PCWSTR::from_raw(target.as_ptr()),
                CRED_TYPE_GENERIC,
                0,
                &mut credential_ptr,
            );

            if result.is_err() {
                let error = windows::Win32::Foundation::GetLastError();
                if error == ERROR_NOT_FOUND {
                    return Err(KeyError::NotFound);
                }
                return Err(KeyError::CredentialManager(format!(
                    "CredReadW failed: {:?}",
                    error
                )));
            }

            let credential = &*credential_ptr;

            if credential.CredentialBlobSize as usize != SECRET_KEY_LENGTH {
                CredFree(credential_ptr as *mut _);
                return Err(KeyError::InvalidKey("Invalid key size".into()));
            }

            let mut key_bytes = [0u8; SECRET_KEY_LENGTH];
            std::ptr::copy_nonoverlapping(
                credential.CredentialBlob,
                key_bytes.as_mut_ptr(),
                SECRET_KEY_LENGTH,
            );

            // Free the credential
            CredFree(credential_ptr as *mut _);

            // ed25519-dalek 2.x: from_bytes doesn't return Result
            Ok(SigningKey::from_bytes(&key_bytes))
        }
    }

    /// Store private key in Windows Credential Manager.
    fn store_private_key(key: &SigningKey) -> Result<(), KeyError> {
        unsafe {
            let mut target: Vec<u16> = CREDENTIAL_TARGET
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut username: Vec<u16> =
                "OwnMon".encode_utf16().chain(std::iter::once(0)).collect();

            let key_bytes = key.to_bytes();

            let credential = CREDENTIALW {
                Flags: CRED_FLAGS(0),
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR::from_raw(target.as_mut_ptr()),
                Comment: PWSTR::null(),
                LastWritten: std::mem::zeroed(),
                CredentialBlobSize: SECRET_KEY_LENGTH as u32,
                CredentialBlob: key_bytes.as_ptr() as *mut u8,
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                AttributeCount: 0,
                Attributes: std::ptr::null_mut(),
                TargetAlias: PWSTR::null(),
                UserName: PWSTR::from_raw(username.as_mut_ptr()),
            };

            CredWriteW(&credential, 0)
                .map_err(|e| KeyError::CredentialManager(format!("CredWriteW failed: {}", e)))?;

            Ok(())
        }
    }

    /// Store public key to file for sharing.
    fn store_public_key(key: &VerifyingKey) -> Result<(), KeyError> {
        let path = Self::public_key_path();

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| KeyError::FileSystem(e.to_string()))?;
        }

        let public_key_base64 = base64::engine::general_purpose::STANDARD.encode(key.as_bytes());

        std::fs::write(&path, &public_key_base64)
            .map_err(|e| KeyError::FileSystem(e.to_string()))?;

        tracing::info!("Public key stored at: {}", path.display());
        Ok(())
    }

    /// Delete stored keys (for testing/reset).
    #[allow(dead_code)]
    pub fn delete_keys() -> Result<(), KeyError> {
        unsafe {
            let target: Vec<u16> = CREDENTIAL_TARGET
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = CredDeleteW(PCWSTR::from_raw(target.as_ptr()), CRED_TYPE_GENERIC, 0);
        }

        let path = Self::public_key_path();
        let _ = std::fs::remove_file(path);

        Ok(())
    }
}

/// Errors that can occur during key operations.
#[derive(Debug)]
pub enum KeyError {
    NotFound,
    InvalidKey(String),
    CredentialManager(String),
    FileSystem(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::NotFound => write!(f, "Key not found"),
            KeyError::InvalidKey(e) => write!(f, "Invalid key: {}", e),
            KeyError::CredentialManager(e) => write!(f, "Credential Manager error: {}", e),
            KeyError::FileSystem(e) => write!(f, "File system error: {}", e),
        }
    }
}

impl std::error::Error for KeyError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        // Clean up first
        let _ = KeyManager::delete_keys();

        // Generate new keys
        let km = KeyManager::init().expect("Failed to init key manager");

        // Verify public key file exists
        assert!(KeyManager::public_key_path().exists());

        // Verify we can load the same key
        let km2 = KeyManager::init().expect("Failed to reload key manager");
        assert_eq!(km.public_key_base64(), km2.public_key_base64());

        // Clean up
        let _ = KeyManager::delete_keys();
    }
}
