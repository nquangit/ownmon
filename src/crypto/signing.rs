//! Session signing and hash functions.
//!
//! Provides cryptographic signing for activity sessions.

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Compute SHA256 hash of session data.
#[allow(clippy::too_many_arguments)]
pub fn hash_session_data(
    process_name: &str,
    window_title: &str,
    start_time: &str,
    end_time: &str,
    keystrokes: u64,
    clicks: u64,
    scrolls: u64,
    prev_hash: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();

    hasher.update(process_name.as_bytes());
    hasher.update(b"|");
    hasher.update(window_title.as_bytes());
    hasher.update(b"|");
    hasher.update(start_time.as_bytes());
    hasher.update(b"|");
    hasher.update(end_time.as_bytes());
    hasher.update(b"|");
    hasher.update(keystrokes.to_le_bytes());
    hasher.update(b"|");
    hasher.update(clicks.to_le_bytes());
    hasher.update(b"|");
    hasher.update(scrolls.to_le_bytes());

    // Chain to previous hash if exists
    if let Some(prev) = prev_hash {
        hasher.update(b"|");
        hasher.update(prev.as_bytes());
    }

    let result = hasher.finalize();
    hex::encode(result)
}

/// Sign a hash with the signing key.
pub fn sign_hash(hash: &str, key: &SigningKey) -> String {
    let signature: Signature = key.sign(hash.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(signature.to_bytes())
}

/// Verify a signature against a hash.
pub fn verify_signature(hash: &str, signature_b64: &str, key: &VerifyingKey) -> bool {
    let signature_bytes = match base64::engine::general_purpose::STANDARD.decode(signature_b64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };

    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    key.verify(hash.as_bytes(), &signature).is_ok()
}

/// Combined hash and sign for session data.
#[allow(clippy::too_many_arguments)]
pub fn hash_and_sign_session(
    key: &SigningKey,
    process_name: &str,
    window_title: &str,
    start_time: &str,
    end_time: &str,
    keystrokes: u64,
    clicks: u64,
    scrolls: u64,
    prev_hash: Option<&str>,
) -> (String, String) {
    let hash = hash_session_data(
        process_name,
        window_title,
        start_time,
        end_time,
        keystrokes,
        clicks,
        scrolls,
        prev_hash,
    );
    let signature = sign_hash(&hash, key);
    (hash, signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_hash_consistency() {
        let hash1 = hash_session_data(
            "code.exe",
            "main.rs",
            "2024-01-01T10:00:00",
            "2024-01-01T10:30:00",
            100,
            50,
            10,
            None,
        );
        let hash2 = hash_session_data(
            "code.exe",
            "main.rs",
            "2024-01-01T10:00:00",
            "2024-01-01T10:30:00",
            100,
            50,
            10,
            None,
        );
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_changes_with_data() {
        let hash1 = hash_session_data(
            "code.exe",
            "main.rs",
            "2024-01-01T10:00:00",
            "2024-01-01T10:30:00",
            100,
            50,
            10,
            None,
        );
        let hash2 = hash_session_data(
            "code.exe",
            "main.rs",
            "2024-01-01T10:00:00",
            "2024-01-01T10:30:00",
            101,
            50,
            10,
            None, // Changed keystrokes
        );
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_sign_and_verify() {
        let key = SigningKey::generate(&mut OsRng);
        let hash = hash_session_data(
            "code.exe",
            "main.rs",
            "2024-01-01T10:00:00",
            "2024-01-01T10:30:00",
            100,
            50,
            10,
            None,
        );

        let signature = sign_hash(&hash, &key);
        assert!(verify_signature(&hash, &signature, &key.verifying_key()));
    }

    #[test]
    fn test_invalid_signature() {
        let key1 = SigningKey::generate(&mut OsRng);
        let key2 = SigningKey::generate(&mut OsRng);
        let hash = "test_hash";

        let signature = sign_hash(hash, &key1);
        // Verify with different key should fail
        assert!(!verify_signature(hash, &signature, &key2.verifying_key()));
    }
}
