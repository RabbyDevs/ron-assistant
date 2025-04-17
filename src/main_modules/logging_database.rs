use async_channel::Receiver;
use bincode;
use serde::{Deserialize, Serialize};
use serenity::all::MessageId;
use sled::Db;
use std::{
    collections::HashSet,
    error::Error,
    fmt,
    sync::Arc,
};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
pub enum LogType {
    Game,
    Discord,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
pub enum InfractionType {
    Ban,     // ban, gameban, game ban
    TempBan, // tempban, temp ban, temporary ban, temporary game ban, temp gameban
    Kick,    // kickwarn, warnkick
    Mute,
    Warn, // barn, warn, marm, warm, worm
    Unknown,
}

impl fmt::Display for LogType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogType::Game => write!(f, "Game"),
            LogType::Discord => write!(f, "Discord"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum SpecialMessageId {
    Text(String),
    Number(u64),
}

impl fmt::Display for InfractionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InfractionType::Ban => write!(f, "Ban"),
            InfractionType::TempBan => write!(f, "Temporary Ban"),
            InfractionType::Kick => write!(f, "Kick"),
            InfractionType::Mute => write!(f, "Mute"),
            InfractionType::Warn => write!(f, "Warn"),
            InfractionType::Unknown => write!(f, "Unknown"),
        }
    }
}

pub type BoxedError = Box<dyn Error + Send + Sync>;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Log {
    pub log_type: LogType,
    pub infraction_type: InfractionType,
    pub roblox_user_ids: Vec<u64>,
    pub discord_user_ids: Vec<u64>,
    pub reason: String,
    pub message_id: u64,
    pub channel_id: u64,
}

pub struct LoggingDB {
    pub db: Db,
}

impl Default for LoggingDB {
    fn default() -> Self {
        Self::new()
    }
}

impl LoggingDB {
    pub fn new() -> Self {
        let db = sled::open("./dbs/logging_db").unwrap();
        LoggingDB { db }
    }

    pub async fn get_by_id(&self, user_id: u64) -> Vec<Log> {
        let id_tree = self.db.open_tree("id_tree").unwrap();
        let main_tree = self.db.open_tree("main_tree").unwrap();

        let user_id_bytes = user_id.to_be_bytes();
        match id_tree.get(user_id_bytes) {
            Ok(Some(ids_bytes)) => {
                let ids: HashSet<u64> = bincode::deserialize(&ids_bytes).unwrap_or_default();
                
                println!("{:#?}", ids);
                let mut logs: Vec<Log> = ids
                    .into_iter()
                    .filter_map(|id| {
                        main_tree.get(id.to_be_bytes()).ok()?.and_then(|log_bytes| {
                            bincode::deserialize(&log_bytes).ok()
                        })
                    })
                    .collect();
                println!("{:#?}", logs);
                // Sort by timestamp using the message_id (u64)
                logs.sort_by_key(|log| std::cmp::Reverse(log.message_id));
                logs
            }
            Ok(None) => Vec::new(),
            Err(_) => Vec::new(),
        }
    }

    pub fn get_last_scanned(&self, channel_id: u64) -> Option<u64> {
        let scan_tree = match self.db.open_tree("last_scanned") {
            Ok(tree) => tree,
            Err(e) => {
                eprintln!("Error opening scan tree: {:?}", e);
                return None;
            }
        };

        let key = channel_id.to_be_bytes();
        match scan_tree.get(key) {
            Ok(Some(msg)) => match bincode::deserialize(&msg) {
                Ok(decoded) => Some(decoded),
                Err(e) => {
                    eprintln!("Failed to deserialize message: {:?}", e);
                    None
                }
            },
            Ok(None) => {
                // No entry found for the given channel_id
                None
            }
            Err(e) => {
                eprintln!("Error retrieving message from store: {:?}", e);
                None
            }
        }
    }

    pub fn set_last_scanned(
        &self,
        channel_id: u64,
        message_id: MessageId,
    ) -> Result<(), Box<dyn Error>> {
        let scan_tree = self.db.open_tree("last_scanned")?;
        scan_tree.insert(
            channel_id.to_be_bytes(),
            bincode::serialize(&message_id.get())?,
        )?;
        Ok(())
    }

    pub fn delete(&self, message_id: u64) -> Result<(), Box<dyn Error>> {
        let main_tree = self.db.open_tree("main_tree")?;
        let id_tree = self.db.open_tree("id_tree")?;

        if let Some(log_bytes) = main_tree.get(message_id.to_be_bytes())? {
            let log: Log = bincode::deserialize(&log_bytes)?;

            for discord_id in log.discord_user_ids {
                let mut ids: HashSet<u64> =
                    if let Some(ids_bytes) = id_tree.get(discord_id.to_be_bytes())? {
                        bincode::deserialize(&ids_bytes)?
                    } else {
                        HashSet::new()
                    };
                ids.remove(&message_id);
                if ids.is_empty() {
                    id_tree.remove(discord_id.to_be_bytes())?;
                } else {
                    id_tree
                        .insert(discord_id.to_be_bytes(), bincode::serialize(&ids)?)?;
                }
            }

            for roblox_id in log.roblox_user_ids {
                let mut ids: HashSet<u64> =
                    if let Some(ids_bytes) = id_tree.get(roblox_id.to_be_bytes())? {
                        bincode::deserialize(&ids_bytes)?
                    } else {
                        HashSet::new()
                    };
                ids.remove(&message_id);
                if ids.is_empty() {
                    id_tree.remove(roblox_id.to_be_bytes())?;
                } else {
                    id_tree
                        .insert(roblox_id.to_be_bytes(), bincode::serialize(&ids)?)?;
                }
            }

        }

        main_tree.remove(message_id.to_be_bytes())?;

        Ok(())
    }

    pub async fn save_single(&self, log: Log) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let main_tree = db.open_tree("main_tree")?;
            let id_tree = db.open_tree("id_tree")?;

            // Use the u64 message_id for the key
            let message_id = log.message_id;
            let message_id_bytes = message_id.to_be_bytes();
            main_tree.insert(message_id_bytes, bincode::serialize(&log)?)?;

            // Update or create entries in the ID trees for Discord and Roblox user IDs
            for discord_id in log.discord_user_ids {
                let key = discord_id.to_be_bytes().to_vec();
                let existing: HashSet<u64> = match id_tree.get(&key)? {
                    Some(bytes) => bincode::deserialize(&bytes)?,
                    None => HashSet::new(),
                };
                let mut new_set = existing;
                new_set.insert(message_id);
                id_tree.insert(&key, bincode::serialize(&new_set)?)?;
            }

            for roblox_id in log.roblox_user_ids {
                let key = roblox_id.to_be_bytes().to_vec();
                let existing: HashSet<u64> = match id_tree.get(&key)? {
                    Some(bytes) => bincode::deserialize(&bytes)?,
                    None => HashSet::new(),
                };
                let mut new_set = existing;
                new_set.insert(message_id);
                id_tree.insert(&key, bincode::serialize(&new_set)?)?;
            }

            Ok(())
        }).await?
    }
    
    // Updated method to save each log individually
    pub async fn save_bulk(
        db: &Arc<Self>,
        receiver: Receiver<Log>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        while let Ok(log) = receiver.recv().await {
            db.save_single(log).await?;
        }
        Ok(())
    }
}
