use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};
use sled::Db;
use futures::future::BoxFuture;
use serde::{Serialize, Deserialize};


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimerData {
    pub timer_id: String,
    pub role_id: String,
    pub end_timestamp: u64,
    pub is_paused: bool,
    pub paused_duration: u64,
    pub schema_version: u32,
    pub delete_on_ban: bool,  // New field
}

#[derive(Clone, Debug)]
pub struct UserTimer {
    end_time: Instant,
    role_id: String,
    paused_at: Option<Instant>,
    paused_duration: Duration,
    delete_on_ban: bool,  // New field
}

type EventHandler = Arc<Mutex<Box<dyn Fn(String, String) -> BoxFuture<'static, ()> + Send + Sync>>>;

pub struct TimerSystem {
    db: Arc<Db>,
    timers: Arc<Mutex<HashMap<String, HashMap<String, UserTimer>>>>,
    event_handler: EventHandler,
}

impl TimerSystem {
    pub async fn new(db_path: &str) -> sled::Result<Self> {
        println!("Initializing TimerSystem with database at: {}", db_path);
        let db = Arc::new(sled::open(db_path)?);
        let timers = Arc::new(Mutex::new(HashMap::new()));
        let event_handler: EventHandler = 
            Arc::new(Mutex::new(Box::new(|_: String, _: String| Box::pin(async {}))));
        let system = TimerSystem {
            db: Arc::clone(&db),
            timers: Arc::clone(&timers),
            event_handler,
        };

        println!("Starting database migration check...");
        if let Err(e) = system.migrate_database().await {
            println!("Error during migration: {:?}", e);
        }

        Ok(system)
    }

    async fn migrate_database(&self) -> sled::Result<()> {
        let mut keys_to_migrate = Vec::new();
        let mut keys_to_delete = Vec::new();
        let mut migration_count = 0;

        println!("Scanning database for entries requiring migration...");

        // First pass: Identify old format data
        for result in self.db.iter() {
            let (key, value) = result?;
            let key_str = String::from_utf8_lossy(&key);

            if !key_str.contains(':') {
                println!("Found old format entry with key: {}", key_str);
                keys_to_migrate.push((key.to_vec(), value.to_vec()));
                keys_to_delete.push(key.to_vec());
                migration_count += 1;
            }
        }

        println!("Found {} entries requiring migration", migration_count);

        // Second pass: Migrate old format data
        for (key, value) in keys_to_migrate {
            let user_id = String::from_utf8_lossy(&key).to_string();
            println!("Migrating timer for user: {}", user_id);
            
            if let Some(migrated_data) = self.migrate_old_format(&user_id, &value)? {
                println!("Successfully migrated timer: {:?}", migrated_data);
                let timer_id = migrated_data.timer_id.clone();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap();
                
                if migrated_data.end_timestamp > now.as_secs() {
                    let duration = Duration::from_secs(migrated_data.end_timestamp - now.as_secs());
                    
                    let timer = UserTimer {
                        end_time: Instant::now() + duration,
                        role_id: migrated_data.role_id.clone(),
                        paused_at: if migrated_data.is_paused { Some(Instant::now()) } else { None },
                        paused_duration: Duration::from_secs(migrated_data.paused_duration),
                        delete_on_ban: true,
                    };

                    println!("Successfully converted user timer, {:#?}", timer);

                    // Asynchronously update the in-memory timers
                    let mut timers = self.timers.lock().await;
                    let user_timers = timers.entry(user_id.clone()).or_insert_with(HashMap::new);
                    user_timers.insert(timer_id.clone(), timer);
                }
            } else {
                println!("Failed to migrate timer for user: {}", user_id);
            }
        }

        // Clean up: Remove old format data
        for key in keys_to_delete {
            let key_str = String::from_utf8_lossy(&key);
            println!("Removing old format entry: {}", key_str);
            self.db.remove(key)?;
        }

        // Verify and load all existing timers after migration
        println!("Loading all existing timers...");
        for result in self.db.iter() {
            let (key, value) = result?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            
            if key_str.contains(':') {
                match bincode::deserialize::<TimerData>(&value) {
                    Ok(timer_data) => {
                        println!("Loading existing timer: {} for role: {}", timer_data.timer_id, timer_data.role_id);
                        self.load_timer_data(&key_str, timer_data).await?;
                    },
                    Err(e) => {
                        println!("Error deserializing timer data for key {}: {:?}", key_str, e);
                        self.db.remove(key)?;
                    }
                }
            }
        }

        println!("Database migration and loading completed successfully");
        Ok(())
    }

    async fn load_timer_data(&self, key_str: &str, timer_data: TimerData) -> sled::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
        
        if timer_data.end_timestamp > now.as_secs() {
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() == 2 {
                let user_id = parts[0].to_string();
                let duration = Duration::from_secs(timer_data.end_timestamp - now.as_secs());
                
                let timer = UserTimer {
                    end_time: Instant::now() + duration,
                    role_id: timer_data.role_id.clone(),
                    paused_at: if timer_data.is_paused { Some(Instant::now()) } else { None },
                    paused_duration: Duration::from_secs(timer_data.paused_duration),
                    delete_on_ban: timer_data.delete_on_ban,
                };

                // Asynchronously update the in-memory timers
                let mut timers = self.timers.lock().await;
                let user_timers = timers.entry(user_id).or_insert_with(HashMap::new);
                user_timers.insert(timer_data.timer_id.clone(), timer);
            }
        } else {
            self.db.remove(key_str.as_bytes())?;
        }

        Ok(())
    }

    fn migrate_old_format(&self, user_id: &str, value: &[u8]) -> sled::Result<Option<TimerData>> {
        if value.len() < 8 {
            return Ok(None);
        }

        let timestamp = u64::from_be_bytes(value[..8].try_into().unwrap());
        let role_id_end = value.iter().skip(8).position(|&x| x == 0 || x == 1);
        
        if let Some(end_pos) = role_id_end {
            let end_pos = end_pos + 8;
            if let Ok(role_id) = String::from_utf8(value[8..end_pos].to_vec()) {
                let is_paused = value[end_pos] == 1;
                let paused_duration = if value.len() > end_pos + 1 {
                    u64::from_be_bytes(value[end_pos+1..].try_into().unwrap())
                } else {
                    0
                };

                let timer_id = format!("migrated_{}", uuid::Uuid::new_v4());
                
                let timer_data = TimerData {
                    timer_id: timer_id.clone(),
                    role_id,
                    end_timestamp: timestamp,
                    is_paused,
                    paused_duration,
                    schema_version: 2,
                    delete_on_ban: true,  // Default value for migrated timers
                };

                let new_key = format!("{}:{}", user_id, timer_id);
                let new_value = bincode::serialize(&timer_data).unwrap();
                self.db.insert(new_key.as_bytes(), new_value)?;

                return Ok(Some(timer_data));
            }
        }

        Ok(None)
    }

    pub async fn add_timer(
        &self,
        user_id: String,
        role_id: String,
        duration_secs: u64,
        is_paused: bool,
        paused_duration: Option<u64>,
        delete_on_ban: bool,  // New parameter
    ) -> sled::Result<String> {
        let timer_id = uuid::Uuid::new_v4().to_string();
        let end_time = Instant::now() + Duration::from_secs(duration_secs);
        let end_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() + duration_secs;

        let timers = self.timers.clone();
        let db = self.db.clone();
        
        let mut timers = timers.lock().await;
        let user_timers = timers.entry(user_id.clone()).or_insert_with(HashMap::new);
        
        let timer = UserTimer {
            end_time,
            role_id: role_id.clone(),
            paused_at: if is_paused { Some(Instant::now()) } else { None },
            paused_duration: paused_duration.map(Duration::from_secs).unwrap_or(Duration::from_secs(0)),
            delete_on_ban,  // New field
        };

        user_timers.insert(timer_id.clone(), timer);

        let timer_data = TimerData {
            timer_id: timer_id.clone(),
            role_id,
            end_timestamp,
            is_paused,
            paused_duration: paused_duration.unwrap_or(0),
            schema_version: 2,
            delete_on_ban,  // New field
        };

        let db_key = format!("{}:{}", user_id, timer_id);
        let db_value = bincode::serialize(&timer_data).unwrap();
        db.insert(db_key.as_bytes(), db_value)?;

        Ok(timer_id)
    }

    pub async fn toggle_timer(&self, user_id: &str, timer_id: &str) -> Result<Option<String>, String> {
        let mut timers = self.timers.lock().await;
        
        let user_timers = timers.get_mut(user_id)
            .ok_or_else(|| "User not found".to_string())?;
        
        let timer = user_timers.get_mut(timer_id)
            .ok_or_else(|| "Timer not found".to_string())?;
        
        let now = Instant::now();
        let db_key = format!("{}:{}", user_id, timer_id);
        
        let existing_data = self.db.get(db_key.as_bytes())
            .map_err(|_| "Failed to read database".to_string())?
            .ok_or_else(|| "Timer data not found in database".to_string())?;
        
        let mut timer_data: TimerData = bincode::deserialize(&existing_data)
            .map_err(|_| "Failed to deserialize timer data".to_string())?;
        
        // If timer is currently running (not paused), pause it
        if timer.paused_at.is_none() {
            timer.paused_at = Some(now);
            timer_data.is_paused = true;
            timer_data.paused_duration = timer.paused_duration.as_secs();
            
            let db_value = bincode::serialize(&timer_data)
                .map_err(|_| "Failed to serialize timer data".to_string())?;
            
            self.db.insert(db_key.as_bytes(), db_value)
                .map_err(|_| "Failed to update database".to_string())?;
            
            Ok(None) // Return None when pausing
        } 
        // If timer is currently paused, resume it
        else {
            let paused_at = timer.paused_at.unwrap();
            let additional_pause = now.duration_since(paused_at);
            timer.paused_duration += additional_pause;
            timer.end_time += additional_pause;
            timer.paused_at = None;
            
            timer_data.is_paused = false;
            timer_data.paused_duration = timer.paused_duration.as_secs();
            timer_data.end_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map_err(|_| "Failed to calculate system time".to_string())?
                .as_secs() + timer.end_time.duration_since(now).as_secs();
            
            let db_value = bincode::serialize(&timer_data)
                .map_err(|_| "Failed to serialize timer data".to_string())?;
            
            self.db.insert(db_key.as_bytes(), db_value)
                .map_err(|_| "Failed to update database".to_string())?;
            
            Ok(Some(timer.role_id.clone())) // Return Some(role_id) when resuming
        }
    }

    pub async fn list_user_timers(&self, user_id: &str) -> Vec<TimerData> {
        let timers = self.timers.lock().await;
        let mut result = Vec::new();
        
        if let Some(user_timers) = timers.get(user_id) {
            for (timer_id, timer) in user_timers {
                let now = Instant::now();
                let remaining_secs = if timer.end_time > now {
                    timer.end_time.duration_since(now).as_secs()
                } else {
                    0
                };
                
                let timer_data = TimerData {
                    timer_id: timer_id.clone(),
                    role_id: timer.role_id.clone(),
                    end_timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() + remaining_secs,
                    is_paused: timer.paused_at.is_some(),
                    paused_duration: timer.paused_duration.as_secs(),
                    schema_version: 2,
                    delete_on_ban: timer.delete_on_ban,  // Include in output
                };
                
                result.push(timer_data);
            }
        }
        
        result
    }

    pub async fn delete_timer(&self, user_id: &str, timer_id: &str) -> Result<(), String> {
        let mut timers = self.timers.lock().await;
        
        if let Some(user_timers) = timers.get_mut(user_id) {
            if user_timers.remove(timer_id).is_some() {
                let db_key = format!("{}:{}", user_id, timer_id);
                match self.db.remove(db_key.as_bytes()) {
                    Ok(_) => {
                        // Remove user entry if no timers left
                        if user_timers.is_empty() {
                            timers.remove(user_id);
                        }
                        Ok(())
                    },
                    Err(e) => Err(format!("Failed to remove timer from database: {}", e)),
                }
            } else {
                Err("Timer not found".to_string())
            }
        } else {
            Err("User not found".to_string())
        }
    }

    pub async fn set_event_handler<F, Fut>(&self, handler: F)
    where
        F: Fn(String, String) -> Fut + Send + Sync + 'static,
        Fut: futures::Future<Output = ()> + Send + 'static,
    {
        *self.event_handler.lock().await = Box::new(move |user_id, role_id| 
            Box::pin(handler(user_id, role_id))
        );
    }

    pub fn start_timer_thread(&self) {
        let timers = Arc::clone(&self.timers);
        let db = Arc::clone(&self.db);
        let event_handler = Arc::clone(&self.event_handler);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let mut timers = timers.lock().await;
                let now = Instant::now();
                
                let mut expired_timers = Vec::new();
                
                // Collect expired timers
                for (user_id, user_timers) in timers.iter() {
                    for (timer_id, timer) in user_timers.iter() {
                        if timer.paused_at.is_none() && timer.end_time <= now {
                            println!("Timer expired:");
                            println!("  User ID: {}", user_id);
                            println!("  Timer ID: {}", timer_id);
                            println!("  Role ID: {}", timer.role_id);
                            println!("  Total pause duration: {:?}", timer.paused_duration);
                            println!("  End time reached at: {:?}", timer.end_time);
                            
                            expired_timers.push((
                                user_id.clone(),
                                timer_id.clone(),
                                timer.role_id.clone()
                            ));
                        }
                    }
                }

                // Handle expired timers
                for (user_id, timer_id, role_id) in &expired_timers {
                    let db_key = format!("{}:{}", user_id, timer_id);
                    match db.remove(db_key.as_bytes()) {
                        Ok(_) => println!("Successfully removed expired timer from database: {}", db_key),
                        Err(e) => println!("Failed to remove expired timer from database: {}", e),
                    }
                    
                    println!("Triggering event handler for expired timer - User: {}, Role: {}", user_id, role_id);
                    let handler = event_handler.lock().await;
                    handler(user_id.clone(), role_id.clone()).await;
                }

                // Remove expired timers from memory
                for (user_id, timer_id, _) in expired_timers {
                    if let Some(user_timers) = timers.get_mut(&user_id) {
                        user_timers.remove(&timer_id);
                        println!("Removed expired timer from memory - User: {}, Timer: {}", user_id, timer_id);
                        if user_timers.is_empty() {
                            timers.remove(&user_id);
                            println!("Removed user entry as no timers remain - User: {}", user_id);
                        }
                    }
                }

                // Update remaining timers in the database
                for (user_id, user_timers) in timers.iter() {
                    for (timer_id, timer) in user_timers.iter() {
                        if timer.paused_at.is_none() && timer.end_time > now {
                            let remaining = timer.end_time - now;
                            let end_timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() + remaining.as_secs();

                            let timer_data = TimerData {
                                timer_id: timer_id.clone(),
                                role_id: timer.role_id.clone(),
                                end_timestamp,
                                is_paused: false,
                                paused_duration: timer.paused_duration.as_secs(),
                                schema_version: 2,
                                delete_on_ban: timer.delete_on_ban
                            };

                            let db_key = format!("{}:{}", user_id, timer_id);
                            let db_value = bincode::serialize(&timer_data).unwrap();
                            db.insert(db_key.as_bytes(), db_value).unwrap();
                        }
                    }
                }
            }
        });
    }
}