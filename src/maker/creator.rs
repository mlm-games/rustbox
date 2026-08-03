//! Anonymous creator identity for online level publishing.
//!
//! The identity *is* the account, the same pattern friend codes and
//! anonymous-first apps use:
//!
//! - A random 32-byte **recovery key** (shown to the player as a portable
//!   `rbx1_...` string) grants ownership and carries the weekly upload quota.
//!   Pasting it on another device restores both ("login elsewhere").
//!   The encoded key carries a 2-byte checksum so a mistyped copy fails
//!   loudly instead of silently becoming a different identity.
//! - A local-only **device id** (random 32 bytes, sent as its sha256 hex) is
//!   a secondary abuse signal, regenerated on import so each device is its own.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use super::storage::LevelStorage;

/// Storage key, `__`-prefixed so it never shows up as a level slot.
pub const CREATOR_STORAGE_KEY: &str = "__creator_key";

/// Prefix that makes a recovery key recognizable when read aloud.
pub const RECOVERY_KEY_PREFIX: &str = "rbx1_";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatorIdentity {
    /// Portable ownership credential the player can copy/import.
    pub recovery_key: String,
    /// 64-hex sha256 of a per-device random secret (abuse signal only).
    pub device_id: String,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    recovery_key: String,
    device_id: String,
}

/// Stable display handle that matches the server's `owner_id_short` (first 10
/// hex chars of `sha256(recovery key bytes)`).
pub fn short_maker_id(recovery_key: &str) -> String {
    let secret = decode_secret(strip_prefix(recovery_key)).unwrap_or_default();
    hex::encode(sha2::Sha256::digest(&secret))
        .chars()
        .take(10)
        .collect()
}

/// Load the persisted identity, or silently generate + persist one on first
/// run (no UI, no registration).
pub fn load_or_create(storage: &LevelStorage) -> anyhow::Result<CreatorIdentity> {
    if let Ok(Some(raw)) = storage.0.load(CREATOR_STORAGE_KEY)
        && let Ok(id) = serde_json::from_str::<Stored>(&raw)
        && id.recovery_key.starts_with(RECOVERY_KEY_PREFIX)
        && id.device_id.len() == 64
        && let Ok(secret) = decode_secret(strip_prefix(&id.recovery_key))
    {
        return Ok(CreatorIdentity {
            // Re-encode so a stored key is always canonical (same owner_id).
            recovery_key: format!("{RECOVERY_KEY_PREFIX}{}", encode_secret(&secret)),
            device_id: id.device_id,
        });
    }
    let id = CreatorIdentity {
        recovery_key: format!("{RECOVERY_KEY_PREFIX}{}", encode_secret(&new_secret())),
        device_id: hex::encode(sha2::Sha256::digest(new_secret())),
    };
    persist(storage, &id)?;
    Ok(id)
}

/// Restore an identity from a pasted recovery key (keeps this device's own
/// device id; the ownership + quota follow the key). Overwrites the stored
/// identity on success.
pub fn import_recovery_key(storage: &LevelStorage, code: &str) -> anyhow::Result<CreatorIdentity> {
    let secret = decode_secret(&normalize_code(code)).map_err(anyhow::Error::msg)?;
    let id = CreatorIdentity {
        recovery_key: format!("{RECOVERY_KEY_PREFIX}{}", encode_secret(&secret)),
        device_id: hex::encode(sha2::Sha256::digest(new_secret())),
    };
    persist(storage, &id)?;
    Ok(id)
}

fn persist(storage: &LevelStorage, id: &CreatorIdentity) -> anyhow::Result<()> {
    let raw = serde_json::to_string(&Stored {
        recovery_key: id.recovery_key.clone(),
        device_id: id.device_id.clone(),
    })?;
    storage.0.save(CREATOR_STORAGE_KEY, &raw)
}

/// Trim whitespace and strip the `rbx1_` prefix so pasted codes (possibly with
/// stray spaces from wrapping) still parse.
fn normalize_code(code: &str) -> String {
    let s: String = code.trim().chars().filter(|c| !c.is_whitespace()).collect();
    match s.strip_prefix(RECOVERY_KEY_PREFIX) {
        Some(rest) => rest.to_string(),
        None => s,
    }
}

fn strip_prefix(s: &str) -> &str {
    s.strip_prefix(RECOVERY_KEY_PREFIX).unwrap_or(s)
}

/// Encode a 32-byte secret as the portable recovery key body: URL-safe base64
/// of the secret plus a 2-byte checksum.
fn encode_secret(secret: &[u8; 32]) -> String {
    let mut payload = [0u8; 34];
    payload[..32].copy_from_slice(secret);
    payload[32..].copy_from_slice(&checksum(secret));
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload)
}

/// Checksum over the secret: first 2 bytes of `sha256(secret)`.
fn checksum(secret: &[u8; 32]) -> [u8; 2] {
    let mut out = [0u8; 2];
    out.copy_from_slice(&sha2::Sha256::digest(secret)[..2]);
    out
}

/// Decode a recovery key body back into its 32 secret bytes, verifying the
/// 2-byte checksum. `Err` (with a human-readable message) on any failure so a
/// mistyped code never silently becomes a different identity.
fn decode_secret(s: &str) -> Result<[u8; 32], &'static str> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| "code contains invalid characters")?;
    if bytes.len() != 34 {
        return Err("code has the wrong length");
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&bytes[..32]);
    if &bytes[32..] != &checksum(&secret) {
        return Err("checksum failed, the code was mistyped or corrupted");
    }
    Ok(secret)
}

fn new_secret() -> [u8; 32] {
    let mut out = [0u8; 32];
    getrandom::fill(&mut out).expect("crypto rng unavailable");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maker::storage::StorageBackend;
    use std::sync::Mutex;

    struct MemBackend(Mutex<std::collections::HashMap<String, String>>);
    impl StorageBackend for MemBackend {
        fn save(&self, key: &str, data: &str) -> anyhow::Result<()> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_string(), data.to_string());
            Ok(())
        }
        fn load(&self, key: &str) -> anyhow::Result<Option<String>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn list(&self) -> anyhow::Result<Vec<String>> {
            Ok(self.0.lock().unwrap().keys().cloned().collect())
        }
        fn delete(&self, key: &str) -> anyhow::Result<()> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn mem() -> LevelStorage {
        LevelStorage(Box::new(MemBackend(Mutex::new(Default::default()))))
    }

    #[test]
    fn load_or_create_persists_and_round_trips() {
        let s = mem();
        let a = load_or_create(&s).unwrap();
        assert!(a.recovery_key.starts_with(RECOVERY_KEY_PREFIX));
        assert_eq!(a.device_id.len(), 64);
        let b = load_or_create(&s).unwrap();
        assert_eq!(a, b, "second load must return the same identity");
    }

    #[test]
    fn import_restores_ownership() {
        let s = mem();
        let original = load_or_create(&s).unwrap();
        // A fresh device importing the same key keeps the owner, new device id.
        let s2 = mem();
        let imported = import_recovery_key(&s2, &original.recovery_key).unwrap();
        assert_eq!(imported.recovery_key, original.recovery_key);
        assert_ne!(imported.device_id, original.device_id);
        assert_eq!(
            short_maker_id(&imported.recovery_key),
            short_maker_id(&original.recovery_key)
        );
    }

    #[test]
    fn import_is_forgiving_of_whitespace() {
        let s = mem();
        let original = load_or_create(&s).unwrap();
        // A pasted code with stray spaces (e.g. wrapped across lines).
        let messy = format!(
            "{} {}",
            &original.recovery_key[..9],
            &original.recovery_key[9..]
        );
        let s2 = mem();
        let imported = import_recovery_key(&s2, &messy).unwrap();
        assert_eq!(imported.recovery_key, original.recovery_key);
    }

    #[test]
    fn import_rejects_garbage() {
        let s = mem();
        assert!(import_recovery_key(&s, "not a key").is_err());
        assert!(import_recovery_key(&s, &"A".repeat(30)).is_err());
    }

    #[test]
    fn import_rejects_mistyped_key() {
        let s = mem();
        let original = load_or_create(&s).unwrap();
        // Flip one character in the encoded payload: same length, still
        // base64-valid, but the checksum no longer verifies.
        let at = 5; // first char after the "rbx1_" prefix
        let flipped = if original.recovery_key.as_bytes()[at] == b'A' {
            "B"
        } else {
            "A"
        };
        let mut mangled = original.recovery_key.clone();
        mangled.replace_range(at..at + 1, flipped);
        let s2 = mem();
        let err = import_recovery_key(&s2, &mangled).unwrap_err();
        assert!(err.to_string().contains("mistyped"));
    }

    #[test]
    fn generated_key_round_trips_via_import() {
        let s = mem();
        let original = load_or_create(&s).unwrap();
        let s2 = mem();
        let imported = import_recovery_key(&s2, &original.recovery_key).unwrap();
        assert_eq!(imported.recovery_key, original.recovery_key);
        assert_eq!(
            short_maker_id(&imported.recovery_key),
            short_maker_id(&original.recovery_key)
        );
    }
}
