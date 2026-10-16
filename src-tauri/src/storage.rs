use crate::models::{Activity, Approval, Settings};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub struct Store {
    conn: Connection,
    cipher: Aes256Gcm,
}
impl Store {
    pub fn open(path: &Path, key: &[u8]) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,payload BLOB NOT NULL,PRIMARY KEY(kind,id)); CREATE TABLE IF NOT EXISTS sends(message_id TEXT PRIMARY KEY,status TEXT NOT NULL,provider_id TEXT);").map_err(|e|e.to_string())?;
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "Invalid encryption key")?;
        Ok(Self { conn, cipher })
    }
    fn encrypt<T: Serialize>(&self, v: &T) -> Result<Vec<u8>, String> {
        let plain = serde_json::to_vec(v).map_err(|e| e.to_string())?;
        let mut nonce = [0u8; 12];
        rand::fill(&mut nonce);
        let encrypted = self
            .cipher
            .encrypt(&Nonce::from(nonce), plain.as_ref())
            .map_err(|_| "Encryption failed")?;
        Ok([nonce.to_vec(), encrypted].concat())
    }
    fn decode<T: DeserializeOwned>(&self, bytes: Vec<u8>) -> Result<T, String> {
        if bytes.len() < 28 {
            return Err("Encrypted record is damaged".into());
        }
        let plain = self
            .cipher
            .decrypt(
                &Nonce::try_from(&bytes[..12]).map_err(|_| "Encrypted record is damaged")?,
                &bytes[12..],
            )
            .map_err(|_| "Could not decrypt local data")?;
        serde_json::from_slice(&plain).map_err(|e| e.to_string())
    }
    pub fn put<T: Serialize>(&self, kind: &str, id: &str, v: &T) -> Result<(), String> {
        self.conn.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![kind,id,self.encrypt(v)?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn put_batch(&self, records: &[(&str, &str, serde_json::Value)]) -> Result<(), String> {
        let encrypted = records
            .iter()
            .map(|(kind, id, value)| Ok((*kind, *id, self.encrypt(value)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        for (kind, id, payload) in encrypted {
            tx.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![kind,id,payload]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<Option<T>, String> {
        let bytes: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT payload FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        bytes.map(|b| self.decode(b)).transpose()
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT payload FROM records WHERE kind=?1 ORDER BY rowid DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([kind], |r| r.get::<_, Vec<u8>>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| self.decode(r.map_err(|e| e.to_string())?))
            .collect()
    }
    pub fn delete(&self, kind: &str, id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM records WHERE kind=?1 AND id=?2",
                params![kind, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn clear(&self, kind: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM records WHERE kind=?1", [kind])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn settings(&self) -> Result<Settings, String> {
        Ok(self.get("settings", "current")?.unwrap_or_default())
    }
    pub fn claim_action(&self, key: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO sends(message_id,status) VALUES(?1,'uncertain')",
                [key],
            )
            .map_err(|_| {
                "This action already has an attempt. Check its outcome before retrying.".to_string()
            })?;
        Ok(())
    }
    pub fn finish_action(
        &self,
        key: &str,
        id: &str,
        value: &serde_json::Value,
    ) -> Result<(), String> {
        let encrypted = self.encrypt(value)?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute("UPDATE sends SET status='sent' WHERE message_id=?1", [key])
            .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO records VALUES('plan-result',?1,?2) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![id,encrypted]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn claim_reply(&self, item: &Approval) -> Result<(), String> {
        let payload = self.encrypt(item)?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sends(message_id,status) VALUES(?1,'uncertain')",
            [&item.message.id],
        )
        .map_err(|_| {
            "This message already has a send attempt. Reconcile its outcome before trying again."
                .to_string()
        })?;
        tx.execute("INSERT INTO records VALUES('approval',?1,?2) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![item.id,payload]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn finish_reply(
        &self,
        item: &Approval,
        activity: &Activity,
        provider_id: Option<&str>,
    ) -> Result<(), String> {
        let approval = self.encrypt(item)?;
        let audit = self.encrypt(activity)?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        if let Some(provider_id) = provider_id {
            tx.execute(
                "UPDATE sends SET status='sent',provider_id=?2 WHERE message_id=?1",
                params![item.message.id, provider_id],
            )
            .map_err(|e| e.to_string())?;
        }
        for (kind, id, payload) in [
            ("approval", &item.id, approval),
            ("activity", &activity.id, audit),
        ] {
            tx.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![kind,id,payload]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    #[cfg(test)]
    pub fn claim_send(&self, message_id: &str) -> Result<(), String> {
        self.conn.execute("INSERT INTO sends(message_id,status) VALUES(?1,'uncertain')",[message_id]).map_err(|_|"This message already has a send attempt. Reconcile its outcome before trying again.".to_string())?;
        Ok(())
    }
    pub fn send_status(&self, message_id: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT status FROM sends WHERE message_id=?1",
                [message_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    #[cfg(test)]
    pub fn finish_send(&self, message_id: &str, provider_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE sends SET status='sent',provider_id=?2 WHERE message_id=?1",
                params![message_id, provider_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn reset(&self) -> Result<(), String> {
        self.conn
            .execute_batch("DELETE FROM records;")
            .map_err(|e| e.to_string())?;
        Ok(())
    } // Send tombstones survive reset: deleting history must not enable duplicate replies.
}
// ---- Secrets ------------------------------------------------------------------------------------
// macOS asks the user to approve every Keychain read by an app whose signature it does not
// recognise, which is every development build and any build not signed with a stable certificate.
// So the Keychain holds exactly one item, the key that encrypts local data. Sign-ins and API keys
// live in an encrypted file beside the database, and that key is read once per launch.

fn keychain_get(name: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new("com.nagiflow.tomo", name).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Keychain: {e}")),
    }
}
fn keychain_set(name: &str, value: &str) -> Result<(), String> {
    keyring::Entry::new("com.nagiflow.tomo", name)
        .map_err(|e| e.to_string())?
        .set_password(value)
        .map_err(|e| format!("Keychain: {e}"))
}
fn keychain_delete(name: &str) {
    if let Ok(entry) = keyring::Entry::new("com.nagiflow.tomo", name) {
        let _ = entry.delete_credential();
    }
}
fn owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}
/// Sign-ins and API keys, kept in memory and mirrored to one AES-256-GCM encrypted file.
/// Without a file (unit tests) it is memory only.
#[derive(Default)]
struct Vault {
    file: Option<(std::path::PathBuf, Aes256Gcm)>,
    values: std::collections::HashMap<String, String>,
}
impl Vault {
    fn open(path: &Path, key: &[u8]) -> Result<Self, String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "Invalid encryption key")?;
        let values = match std::fs::read(path) {
            Ok(bytes) if bytes.len() >= 28 => {
                let plain = cipher
                    .decrypt(
                        &Nonce::try_from(&bytes[..12])
                            .map_err(|_| "Encrypted record is damaged")?,
                        &bytes[12..],
                    )
                    .map_err(|_| "Could not decrypt saved sign-ins")?;
                serde_json::from_slice(&plain).map_err(|e| e.to_string())?
            }
            Ok(_) => return Err("Saved sign-ins are damaged".into()),
            Err(_) => Default::default(),
        };
        Ok(Self {
            file: Some((path.to_path_buf(), cipher)),
            values,
        })
    }
    fn save(&self) -> Result<(), String> {
        let Some((path, cipher)) = &self.file else {
            return Ok(());
        };
        let mut nonce = [0u8; 12];
        rand::fill(&mut nonce);
        let plain = serde_json::to_vec(&self.values).map_err(|e| e.to_string())?;
        let encrypted = cipher
            .encrypt(&Nonce::from(nonce), plain.as_ref())
            .map_err(|_| "Encryption failed")?;
        // Write beside the file and rename, so a crash mid-write cannot lose every sign-in.
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, [nonce.to_vec(), encrypted].concat()).map_err(|e| e.to_string())?;
        owner_only(&temp);
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    }
}
fn vault() -> std::sync::MutexGuard<'static, Vault> {
    static VAULT: std::sync::OnceLock<std::sync::Mutex<Vault>> = std::sync::OnceLock::new();
    VAULT
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
/// Opens the secrets file at startup. The first time, it also moves anything an earlier version
/// left as separate Keychain items into the file, then removes those items.
pub fn open_vault(dir: &Path, key: &[u8], earlier_items: &[String]) -> Result<(), String> {
    let path = dir.join("secrets.enc");
    let first_time = !path.exists();
    let mut opened = Vault::open(&path, key)?;
    if first_time {
        for name in earlier_items {
            if let Ok(Some(value)) = keychain_get(name) {
                opened.values.insert(name.clone(), value);
            }
        }
        opened.save()?;
        for name in opened.values.keys() {
            keychain_delete(name);
        }
    }
    *vault() = opened;
    Ok(())
}
pub fn credential_get(name: &str) -> Result<Option<String>, String> {
    Ok(vault().values.get(name).cloned())
}
pub fn credential_set(name: &str, value: &str) -> Result<(), String> {
    let mut vault = vault();
    vault.values.insert(name.into(), value.into());
    vault.save()
}
pub fn credential_delete(name: &str) -> Result<(), String> {
    let mut vault = vault();
    if vault.values.remove(name).is_some() {
        vault.save()?;
    }
    Ok(())
}
/// The key that encrypts the database and the secrets file.
/// Release builds keep it in the Keychain: one item, read once per launch.
/// Development builds keep it in a file only this user can read, because macOS would otherwise ask
/// for the Keychain password after every rebuild. A key already in the Keychain is adopted, not replaced.
pub fn database_key(dir: &Path) -> Result<Vec<u8>, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let dev_file = dir.join("dev-database-key");
    if cfg!(debug_assertions) {
        if let Ok(saved) = std::fs::read_to_string(&dev_file) {
            return STANDARD.decode(saved.trim()).map_err(|e| e.to_string());
        }
    }
    let key = match keychain_get("database-key")? {
        Some(saved) => STANDARD.decode(saved).map_err(|e| e.to_string())?,
        None => {
            let mut key = vec![0u8; 32];
            rand::fill(&mut key);
            if !cfg!(debug_assertions) {
                keychain_set("database-key", &STANDARD.encode(&key))?;
            }
            key
        }
    };
    if cfg!(debug_assertions) {
        std::fs::write(&dev_file, STANDARD.encode(&key)).map_err(|e| e.to_string())?;
        owner_only(&dev_file);
    }
    Ok(key)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_survive_a_restart_and_need_the_right_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        let mut vault = Vault::open(&path, &[7; 32]).unwrap();
        vault
            .values
            .insert("tokens:siwc".into(), "refresh-token".into());
        vault.save().unwrap();
        let on_disk = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&on_disk).contains("refresh-token"));
        let reopened = Vault::open(&path, &[7; 32]).unwrap();
        assert_eq!(reopened.values["tokens:siwc"], "refresh-token");
        assert!(Vault::open(&path, &[8; 32]).is_err());
    }
    #[test]
    fn encryption_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        {
            let s = Store::open(&path, &[42; 32]).unwrap();
            s.put("message", "1", &"private contract discussion")
                .unwrap();
            assert_eq!(
                s.get::<String>("message", "1").unwrap().unwrap(),
                "private contract discussion"
            );
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private contract"));
        let s = Store::open(&path, &[42; 32]).unwrap();
        assert!(s.get::<String>("message", "1").unwrap().is_some());
        let wrong = Store::open(&path, &[41; 32]).unwrap();
        assert!(wrong.get::<String>("message", "1").is_err());
    }
    #[test]
    fn sends_survive_history_deletion_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        {
            let s = Store::open(&path, &[0; 32]).unwrap();
            s.claim_send("abc").unwrap();
            assert!(s.claim_send("abc").is_err());
            s.reset().unwrap();
        }
        let s = Store::open(&path, &[0; 32]).unwrap();
        assert!(s.claim_send("abc").is_err());
        s.finish_send("abc", "provider-id").unwrap();
        assert_eq!(s.send_status("abc").unwrap().as_deref(), Some("sent"));
    }
}
#[cfg(test)]
mod send_transaction_tests {
    use super::*;
    #[test]
    fn claim_and_final_audit_recover_together() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sends.db");
        let mut item:Approval=serde_json::from_value(serde_json::json!({"id":"approval","message":{"id":"incoming","provider":"gmail","accountId":"account","conversationId":"thread","sender":{"id":"sender","name":"Sender"},"content":"Original message","timestamp":"2026-10-10T00:00:00Z","isDirectMessage":true,"isMention":false,"requiresProcessing":false,"metadata":{}},"subject":"Test","classification":{"category":"needs_decision","confidence":0.0,"reason":"Manual review","suggestedAction":"require_approval","risk":"high"},"draft":"Test reply","context":[],"status":"sending"})).unwrap();
        {
            let store = Store::open(&path, &[3; 32]).unwrap();
            store.claim_reply(&item).unwrap();
            assert!(store.claim_reply(&item).is_err());
        }
        let store = Store::open(&path, &[3; 32]).unwrap();
        assert_eq!(
            store.send_status("incoming").unwrap().as_deref(),
            Some("uncertain")
        );
        assert_eq!(
            store
                .get::<Approval>("approval", "approval")
                .unwrap()
                .unwrap()
                .draft,
            "Test reply"
        );
        item.status = "sent".into();
        let audit = crate::engine::event(
            &item,
            "approved",
            "User approved",
            true,
            Some("provider-id".into()),
        );
        store
            .finish_reply(&item, &audit, Some("provider-id"))
            .unwrap();
        drop(store);
        let store = Store::open(&path, &[3; 32]).unwrap();
        assert_eq!(
            store.send_status("incoming").unwrap().as_deref(),
            Some("sent")
        );
        assert_eq!(
            store
                .get::<Approval>("approval", "approval")
                .unwrap()
                .unwrap()
                .status,
            "sent"
        );
        assert_eq!(
            store.list::<Activity>("activity").unwrap()[0].action,
            "approved"
        );
    }
}
