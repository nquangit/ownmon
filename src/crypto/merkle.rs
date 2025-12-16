//! Merkle tree implementation for daily integrity verification.
//!
//! Builds a binary Merkle tree from session hashes to create
//! a single root hash representing all activity for a day.

use sha2::{Digest, Sha256};

/// Build a Merkle tree from a list of hashes and return the root.
pub fn build_merkle_root(hashes: &[String]) -> Option<String> {
    if hashes.is_empty() {
        return None;
    }

    if hashes.len() == 1 {
        return Some(hashes[0].clone());
    }

    let mut current_level: Vec<String> = hashes.to_vec();

    while current_level.len() > 1 {
        let mut next_level = Vec::new();

        for chunk in current_level.chunks(2) {
            let combined_hash = if chunk.len() == 2 {
                hash_pair(&chunk[0], &chunk[1])
            } else {
                // Odd number of hashes: duplicate the last one
                hash_pair(&chunk[0], &chunk[0])
            };
            next_level.push(combined_hash);
        }

        current_level = next_level;
    }

    Some(current_level[0].clone())
}

/// Hash two strings together to form a parent node.
fn hash_pair(left: &str, right: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    hex::encode(hasher.finalize())
}

/// Verify that a set of hashes produces the expected root.
pub fn verify_merkle_root(hashes: &[String], expected_root: &str) -> bool {
    match build_merkle_root(hashes) {
        Some(computed_root) => computed_root == expected_root,
        None => expected_root.is_empty(),
    }
}

/// Daily integrity record.
#[derive(Debug, Clone)]
pub struct DailyIntegrity {
    pub date: String,
    pub merkle_root: String,
    pub prev_day_root: Option<String>,
    pub session_count: u32,
    pub signature: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_hashes() {
        assert_eq!(build_merkle_root(&[]), None);
    }

    #[test]
    fn test_single_hash() {
        let hashes = vec!["abc123".to_string()];
        assert_eq!(build_merkle_root(&hashes), Some("abc123".to_string()));
    }

    #[test]
    fn test_two_hashes() {
        let hashes = vec!["hash1".to_string(), "hash2".to_string()];
        let root = build_merkle_root(&hashes).unwrap();
        assert_eq!(root, hash_pair("hash1", "hash2"));
    }

    #[test]
    fn test_four_hashes() {
        let hashes = vec![
            "h1".to_string(),
            "h2".to_string(),
            "h3".to_string(),
            "h4".to_string(),
        ];
        let root = build_merkle_root(&hashes).unwrap();

        // Manual calculation:
        // Level 1: hash(h1,h2), hash(h3,h4)
        // Level 2: hash(level1[0], level1[1])
        let l1_0 = hash_pair("h1", "h2");
        let l1_1 = hash_pair("h3", "h4");
        let expected = hash_pair(&l1_0, &l1_1);

        assert_eq!(root, expected);
    }

    #[test]
    fn test_odd_number_hashes() {
        let hashes = vec!["h1".to_string(), "h2".to_string(), "h3".to_string()];
        let root = build_merkle_root(&hashes);
        assert!(root.is_some());
    }

    #[test]
    fn test_verify_merkle_root() {
        let hashes = vec!["hash1".to_string(), "hash2".to_string()];
        let root = build_merkle_root(&hashes).unwrap();

        assert!(verify_merkle_root(&hashes, &root));
        assert!(!verify_merkle_root(&hashes, "wrong_root"));
    }

    #[test]
    fn test_deterministic() {
        let hashes = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let root1 = build_merkle_root(&hashes);
        let root2 = build_merkle_root(&hashes);
        assert_eq!(root1, root2);
    }
}
