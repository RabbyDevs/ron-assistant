use std::{sync::Arc, time::{Duration, SystemTime}};
use chrono::{DateTime, Utc};

use once_cell::sync::Lazy;
use sled::{Db, Error as SledError};
use serenity::all::{Attachment, MessageId, Timestamp, UserId};
use tokio::sync::Mutex;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AttachmentStore {
    pub message_id: MessageId,
    pub user_id: UserId,
    pub attachments: Vec<Attachment>,
    pub created_at: Timestamp
}

pub struct AttachmentStoreDB {
    db: Db,
}

impl Default for AttachmentStoreDB {
    fn default() -> Self {
        Self::new()
    }
}

impl AttachmentStoreDB {
    pub fn get_instance() -> Arc<Mutex<Self>> {
        ATTACHMENT_STORE_DB.clone()
    }

    pub fn new() -> Self {
        let db = sled::open("./dbs/attachment_logs").unwrap();
        AttachmentStoreDB { db }
    }

    pub fn save(&self, store: &AttachmentStore) -> Result<(), SledError> {
        let key = store.message_id.to_string().into_bytes();
        let value = match serde_json::to_vec(store) {
            Ok(v) => v,
            Err(e) => return Err(SledError::Io(std::io::Error::other(e))),
        };
        self.db.insert(key, value)?;
        Ok(())
    }

    pub fn get(&self, message_id: &str) -> Option<AttachmentStore> {
        let key = message_id.as_bytes();
        self.db.get(key).ok().and_then(|value| {
            value.map(|v| serde_json::from_slice(&v).unwrap())
        })
    }

    pub fn delete(&self, message_id: &str) -> Result<(), SledError> {
        let key = message_id.as_bytes();
        self.db.remove(key)?;
        Ok(())
    }

    pub fn delete_old_entries(&self) -> Result<(), SledError> {
        let seven_days = Duration::from_secs(7 * 24 * 60 * 60);
        let now = SystemTime::now();
        let cutoff = DateTime::<Utc>::from(now.checked_sub(seven_days).unwrap());

        for key in self.db.iter().keys() {
            let key = key?;
            if let Some(store) = self.get(String::from_utf8_lossy(key.as_ref()).as_ref()) {
                if store.created_at < Timestamp::from(cutoff) {
                    self.db.remove(key)?;
                }
            }
        }

        Ok(())
    }
}

static ATTACHMENT_STORE_DB: Lazy<Arc<Mutex<AttachmentStoreDB>>> = Lazy::new(|| {
    Arc::new(Mutex::new(AttachmentStoreDB::new()))
});

pub async fn start_attachment_db() {
    let db = AttachmentStoreDB::get_instance();

    std::thread::spawn(async move || {
        loop {
            std::thread::sleep(Duration::from_secs(1800)); // 30 minutes
            if let Err(e) = db.lock().await.delete_old_entries() {
                eprintln!("Error deleting old attachment entries: {}", e);
            }
        }
    });
}