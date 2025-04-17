#![allow(nonstandard_style)]
use super::logging_database::{self, BoxedError, Log, LoggingDB};
use super::{CONFIG, UserId};
use crate::Data;
use futures::future::join_all;
use regex::Regex;
use reqwest::Client;
use reqwest::header::HeaderValue;
use serde_json::Value;
use serenity::all::{CreateEmbed, CreateEmbedFooter, Message};
use std::collections::HashMap;
use std::fmt::Write;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_segmentation::UnicodeSegmentation;

pub async fn discord_id_to_roblox_id(
    reqwest_client: &Client,
    discord_id: UserId,
) -> Result<String, String> {
    let quote_regex = Regex::new("/\"/gi").expect("regex err");
    let bloxlink_api_key: HeaderValue = CONFIG
        .main
        .bloxlink_api_key
        .parse::<HeaderValue>()
        .expect("err");

    let url = format!(
        "https://api.blox.link/v4/public/discord-to-roblox/{}",
        discord_id
    );
    let response = reqwest_client
        .get(url)
        .header("Authorization", bloxlink_api_key)
        .send()
        .await
        .expect("??");
    if response.status() != reqwest::StatusCode::OK {
        Err(format!(
            "Something went wrong attempting to get Bloxlink data for user `{}`. They might not be verified with Bloxlink.",
            discord_id
        ))
    } else {
        let serialized_json: Value =
            serde_json::from_str(response.text().await.expect("err").as_str()).expect("err");
        Ok(quote_regex
            .replace(serialized_json["robloxID"].as_str().unwrap(), "")
            .to_string())
    }
}

pub async fn duration_conversion(duration_string: String) -> Result<(u64, u64, String), String> {
    let mut date_map = HashMap::new();
    date_map.insert("s", (1, "Second"));
    date_map.insert("h", (3600, "Hour"));
    date_map.insert("d", (86400, "Day"));
    date_map.insert("w", (604800, "Week"));
    date_map.insert("m", (2629743, "Month"));
    date_map.insert("y", (31556952, "Year"));
    let duration_list = duration_string
        .split(' ')
        .map(str::to_string)
        .collect::<Vec<String>>();
    let mut unix_total = 0;
    let mut final_string = String::new();
    if duration_list.is_empty() {
        return Err(format!(
            "Something went wrong parsing duration string `{}`.",
            duration_string
        ));
    } else {
        for duration in duration_list.clone() {
            let chars = duration.chars();
            let amount = match chars
                .clone()
                .filter(|x| x.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
            {
                Ok(amount) => amount,
                Err(_) => {
                    return Err(format!(
                        "Something went wrong parsing duration string `{}`.",
                        duration_string
                    ));
                }
            };
            let identifier = chars.last().expect("err");
            if !date_map.contains_key(identifier.to_string().as_str()) {
                return Err(format!(
                    "Something went wrong parsing duration string `{}`.",
                    duration_string
                ));
            }
            let mut name = date_map[&identifier.to_string().as_str()].1.to_string();
            if amount > 1 {
                name = format!("{} {}s, ", amount, name)
            } else {
                name = format!("{} {}, ", amount, name)
            }
            if duration_list.ends_with(&[duration.clone()]) {
                name.pop();
                name.pop();
            }
            if duration_list.ends_with(&[duration.clone()])
                && !duration_list.starts_with(&[duration.clone()])
            {
                name = format!("and {}", name);
            }
            final_string.push_str(name.as_str());
            let unix_unit = date_map[&identifier.to_string().as_str()].0 * amount;
            unix_total += unix_unit
        }
    }
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let epoch_in_s = since_the_epoch.as_secs();
    Ok((epoch_in_s, epoch_in_s + unix_total, final_string))
}

pub fn format_duration(seconds: u64) -> String {
    let mut date_units = [
        (31556952, "Year"),
        (2629743, "Month"),
        (604800, "Week"),
        (86400, "Day"),
        (3600, "Hour"),
        (60, "Minute"),
        (1, "Second"),
    ];

    let mut remaining_seconds = seconds;
    let mut parts = Vec::new();

    if seconds == 0 {
        return "0 Seconds".to_string();
    }

    for (unit_seconds, unit_name) in date_units.iter_mut() {
        if remaining_seconds >= *unit_seconds {
            let count = remaining_seconds / *unit_seconds;
            remaining_seconds %= *unit_seconds;

            if count > 0 {
                let unit_str = if count == 1 {
                    unit_name.to_string()
                } else {
                    format!("{}s", unit_name)
                };
                parts.push(format!("{} {}", count, unit_str));
            }
        }
    }

    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        _ => {
            let last = parts.pop().unwrap();
            if parts.is_empty() {
                last
            } else {
                format!("{} and {}", parts.join(", "), last)
            }
        }
    }
}

use futures::stream::{self, StreamExt};
use indexmap::IndexMap;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Deserialize)]
struct BadgeResponse {
    nextPageCursor: Option<String>,
    data: Vec<BadgeData>,
}

#[derive(Deserialize)]
struct BadgeData {
    statistics: BadgeStatistics,
    awarder: Awarder,
}

#[derive(Deserialize)]
struct BadgeStatistics {
    winRatePercentage: f64,
}

#[derive(Deserialize)]
struct Awarder {
    id: u64,
}

pub async fn badge_data(
    reqwest_client: &Client,
    roblox_id: String,
    badge_iterations: i64,
) -> Result<(i64, f64, String), String> {
    let badge_count = Arc::new(Mutex::new(0));
    let total_win_rate = Arc::new(Mutex::new(0.0));
    let awarders = Arc::new(Mutex::new(IndexMap::new()));
    let roblox_id = Arc::new(roblox_id);

    let mut cursors = vec![String::new()];
    let mut iteration = 0;

    while iteration < badge_iterations && !cursors.is_empty() {
        let chunk_size = std::cmp::min(cursors.len(), 10);
        let chunk: Vec<_> = cursors.drain(..chunk_size).collect();

        let results = stream::iter(chunk)
            .map(|cursor| {
                let roblox_id = Arc::clone(&roblox_id);
                let badge_count = Arc::clone(&badge_count);
                let total_win_rate = Arc::clone(&total_win_rate);
                let awarders = Arc::clone(&awarders);
                async move {
                    let url = format!(
                        "https://badges.roblox.com/v1/users/{}/badges?limit=100&sortOrder=Asc{}",
                        roblox_id,
                        if cursor.is_empty() {
                            String::new()
                        } else {
                            format!("&cursor={}", cursor)
                        }
                    );

                    let response = reqwest_client
                        .get(&url)
                        .send()
                        .await
                        .map_err(|e| format!("Request failed: {}", e))?;

                    if !response.status().is_success() {
                        return Err(format!("Request failed with status: {}", response.status()));
                    }

                    let text = response
                        .text()
                        .await
                        .map_err(|e| format!("Failed to get response text: {}", e))?;

                    let json: Value = serde_json::from_str(&text)
                        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                    let badge_response: BadgeResponse = serde_json::from_value(json)
                        .map_err(|e| format!("Failed to deserialize BadgeResponse: {}", e))?;

                    let mut badge_count = badge_count.lock().await;
                    *badge_count += badge_response.data.len() as i64;

                    let mut total_win_rate = total_win_rate.lock().await;
                    let mut awarders = awarders.lock().await;

                    for badge in badge_response.data {
                        *total_win_rate += badge.statistics.winRatePercentage;
                        *awarders.entry(badge.awarder.id).or_insert(0) += 1;
                    }

                    Ok(badge_response.nextPageCursor)
                }
            })
            .buffer_unordered(chunk_size)
            .collect::<Vec<_>>()
            .await;

        for result in results {
            match result {
                Ok(Some(next_cursor)) if !next_cursor.is_empty() => {
                    cursors.push(next_cursor);
                }
                Ok(_) => {} // No more pages
                Err(e) => return Err(e),
            }
        }

        iteration += chunk_size as i64;
    }

    let badge_count = *badge_count.lock().await;
    let total_win_rate = *total_win_rate.lock().await;
    let awarders = awarders.lock().await;

    let win_rate = if badge_count > 0 {
        (total_win_rate * 100.0) / badge_count as f64
    } else {
        0.0
    };

    let mut awarders_vec: Vec<_> = awarders.iter().map(|(k, v)| (*k, *v)).collect();
    awarders_vec.sort_unstable_by(|(_, a), (_, b)| b.cmp(a));
    awarders_vec.truncate(5);

    let awarders_string = if awarders_vec.is_empty() {
        "No badges found, there are no top badge givers.".to_string()
    } else {
        awarders_vec
            .iter()
            .fold(String::new(), |mut acc, (id, count)| {
                let _ = write!(acc, "\n - {}: {}", id, count);
                acc
            })
    };

    Ok((badge_count, win_rate, awarders_string))
}

pub async fn roblox_friend_count(
    reqwest_client: &Client,
    roblox_id: &str,
) -> Result<usize, String> {
    let url = format!("https://friends.roblox.com/v1/users/{}/friends", roblox_id);
    let response = reqwest_client.get(&url).send().await.unwrap();
    let response_text = response.text().await.unwrap();

    let parsed_json: Value = serde_json::from_str(&response_text).unwrap();

    Ok(parsed_json["data"]
        .as_array()
        .ok_or_else(|| "Data is not an array".to_string())?
        .len())
}

pub async fn roblox_group_count(reqwest_client: &Client, roblox_id: &str) -> Result<usize, String> {
    let url = format!(
        "https://groups.roblox.com/v2/users/{}/groups/roles?includeLocked=true",
        roblox_id
    );
    let response = reqwest_client.get(&url).send().await.unwrap();
    let response_text = response.text().await.unwrap();

    let parsed_json: Value = serde_json::from_str(&response_text).unwrap();

    Ok(parsed_json["data"]
        .as_array()
        .ok_or_else(|| "Data is not an array".to_string())?
        .len())
}

pub async fn merge_types(
    reqwest_client: &Client,
    rbx_client: &roboat::Client,
    users: Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let mut roblox_ids: Vec<String> = Vec::new();
    let mut errors_vector: Vec<String> = Vec::new();

    for user in users {
        if user.len() >= 17 && user.chars().all(|c| c.is_ascii_digit()) {
            let discord_id = match UserId::from_str(user.as_str()) {
                Ok(id) => id,
                Err(err) => {
                    errors_vector.push(format!("Couldn't find turn discord id string into actual discord id for {}, details:\n{}", user, err));
                    continue;
                }
            };
            let roblox_id_str =
                match self::discord_id_to_roblox_id(reqwest_client, discord_id).await {
                    Ok(id) => id,
                    Err(err) => {
                        errors_vector.push(format!(
                            "Couldn't find turn discord id into roblox id for {}, details:\n{}",
                            user, err
                        ));
                        continue;
                    }
                };
            roblox_ids.push(roblox_id_str)
        } else if user.len() < 17 && user.chars().all(|c| c.is_ascii_digit()) {
            roblox_ids.push(user)
        } else if !user.chars().all(|c| c.is_ascii_digit()) {
            let user_search = match rbx_client
                .username_user_details(vec![user.clone()], false)
                .await
            {
                Ok(id) => id,
                Err(err) => {
                    errors_vector.push(format!(
                        "Couldn't find user details for {}, details:\n{}",
                        user, err
                    ));
                    continue;
                }
            };
            for details in user_search {
                roblox_ids.push(details.id.to_string())
            }
        }
    }
    (roblox_ids, errors_vector)
}

pub async fn split_types(
    rbx_client: &roboat::Client,
    users: Vec<String>,
) -> (Vec<u64>, Vec<u64>, Vec<String>) {
    let mut roblox_ids: Vec<u64> = Vec::new();
    let mut discord_ids: Vec<u64> = Vec::new(); // New vector to store Discord IDs
    let mut errors_vector: Vec<String> = Vec::new();

    for user in users {
        if user.len() >= 17 && user.chars().all(|c| c.is_ascii_digit()) {
            // Case: User is a Discord ID
            match UserId::from_str(user.as_str()) {
                Ok(id) => id,
                Err(err) => {
                    errors_vector.push(format!(
                        "Couldn't turn Discord ID string into actual Discord ID for {}, details:\n{}",
                        user, err
                    ));
                    continue;
                }
            };

            discord_ids.push(u64::from_str(&user).unwrap());
        } else if user.len() < 17 && user.chars().all(|c| c.is_ascii_digit()) {
            roblox_ids.push(u64::from_str(&user).unwrap());
        } else if !user.chars().all(|c| c.is_ascii_digit()) {
            let user_search = match rbx_client
                .username_user_details(vec![user.clone()], false)
                .await
            {
                Ok(details) => details,
                Err(err) => {
                    errors_vector.push(format!(
                        "Couldn't find user details for {}, details:\n{}",
                        user, err
                    ));
                    continue;
                }
            };

            for details in user_search {
                roblox_ids.push(details.id);
            }
        }
    }

    (roblox_ids, discord_ids, errors_vector)
}

pub async fn get_roblox_avatar_bust(reqwest_client: &Client, user_id: String) -> String {
    let response = reqwest_client.get(format!("https://thumbnails.roblox.com/v1/users/avatar-bust?userIds={}&size=420x420&format=Png&isCircular=false", user_id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let parsed_json: Value = serde_json::from_str(response.as_str()).unwrap();
    parsed_json["data"].as_array().unwrap().first().unwrap()["imageUrl"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

pub async fn new_embed_from_template(framework_data: &Data) -> CreateEmbed {
    CreateEmbed::new().color(framework_data.bot_color).footer(
        CreateEmbedFooter::new("Made by RabbyDevs, with 🦀 and ❤️.")
            .icon_url(framework_data.bot_avatar.clone()),
    )
}

pub fn extract_emojis(message_content: &str) -> (Vec<String>, Vec<(String, u64)>) {
    let custom_emoji_regex = regex::Regex::new(r"<:([a-zA-Z0-9_]+):(\d+)>").unwrap();
    let mut custom_emojis = Vec::new();

    let mut processed_content = message_content.to_string();

    for cap in custom_emoji_regex.captures_iter(message_content) {
        if let (Some(name), Some(id_str)) = (cap.get(1), cap.get(2)) {
            if let Ok(emoji_id) = id_str.as_str().parse::<u64>() {
                custom_emojis.push((name.as_str().to_string(), emoji_id));
            }
        }
        if let Some(whole_match) = cap.get(0) {
            processed_content =
                processed_content.replace(whole_match.as_str(), &" ".repeat(whole_match.len()));
        }
    }

    let unicode_emojis: Vec<String> = processed_content
        .graphemes(true)
        .filter(|&g| emojis::get(g).is_some())
        .map(|g| g.to_string())
        .collect();

    (unicode_emojis, custom_emojis)
}

pub async fn get_discord_ids(content: &str) -> Vec<u64> {
    // Initialize result vector
    let mut user_ids = Vec::new();
    
    // Create regex patterns
    let bracket_pattern = Regex::new(r"\[(\d{17,19})\]").unwrap();
    let mention_pattern = Regex::new(r"<@!?(\d{17,19})>").unwrap();
    let raw_id_pattern = Regex::new(r"(?m)^(\d{17,19})$").unwrap();
    
    // Split content into sections by empty lines
    let sections: Vec<&str> = content.split("\n\n").collect();
    
    // Process first section (usually contains command and affected users)
    if let Some(first_section) = sections.first() {
        // Check for bracketed IDs
        for cap in bracket_pattern.captures_iter(first_section) {
            if let Some(id) = cap.get(1) {
                user_ids.push(u64::from_str(id.as_str()).unwrap());
            }
        }
        
        // Check for mention format
        for cap in mention_pattern.captures_iter(first_section) {
            if let Some(id) = cap.get(1) {
                user_ids.push(u64::from_str(id.as_str()).unwrap());
            }
        }
        
        // Check for raw IDs at start of lines
        for cap in raw_id_pattern.captures_iter(first_section) {
            if let Some(id) = cap.get(1) {
                user_ids.push(u64::from_str(id.as_str()).unwrap());
            }
        }
    }
    
    user_ids
}

pub async fn get_roblox_ids(message: &str) -> Vec<u64> {
    let re = Regex::new(r"(?m)\d{6,}").unwrap();
    const DISCORD_ID_LEN: usize = 16;
    
    let mut potential_ids: Vec<u64> = re
        .find_iter(message)
        .map(|m| m.as_str())
        .filter(|s| s.len() < DISCORD_ID_LEN)
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    
    // Remove duplicates
    potential_ids.sort();
    potential_ids.dedup();
    potential_ids
}

pub fn is_valid_discord_id(id: u64) -> bool {
    let id_str = id.to_string();
    if id_str.len() < 18 {
        return false;
    }

    const DISCORD_EPOCH: u64 = 1420070400000;
    const TIMESTAMP_BITS: u64 = 42;

    let timestamp = (id >> (64 - TIMESTAMP_BITS)) + DISCORD_EPOCH;
    
    if let Ok(current_time) = SystemTime::now().duration_since(UNIX_EPOCH) {
        if timestamp < DISCORD_EPOCH || timestamp > current_time.as_millis() as u64 {
            return false;
        }
    }

    true
}

pub fn get_reason(message: &str) -> String {
    // Trim leading and trailing whitespace
    let message = message.trim();
    
    // Find the last occurrence of a number with 6 or more digits
    let mut last_long_number_end = None;
    let mut current_digit_count = 0;
    
    // Iterate through characters to find digit sequences
    // Using chars() instead of as_bytes() to better handle potential Unicode
    for (i, c) in message.chars().enumerate() {
        if c.is_ascii_digit() {
            current_digit_count += 1;
            // If we've found 6 or more digits, update the potential end position
            if current_digit_count >= 6 {
                last_long_number_end = Some(i + 1);
            }
        } else {
            current_digit_count = 0;
        }
    }
    
    let start_index = match last_long_number_end {
        Some(index) => index,
        None => return "reason not found".to_string(),
    };
    
    // Extract everything after the number and trim
    let reason = &message[start_index..];
    
    // Remove square brackets and trim again
    reason.replace(['[', ']'], "").trim().to_string()
}

pub fn get_infraction_type(message: &str) -> logging_database::InfractionType {
    let lower_msg = message.to_lowercase();

    if lower_msg.contains("ban") {
        if lower_msg.contains("temp") || lower_msg.contains("temporary") {
            logging_database::InfractionType::TempBan
        } else if lower_msg.contains("game") || !lower_msg.contains("temp") {
            logging_database::InfractionType::Ban
        } else {
            logging_database::InfractionType::Unknown
        }
    } else if lower_msg.contains("kick") && lower_msg.contains("warn") {
        logging_database::InfractionType::Kick
    } else if lower_msg.contains("mute") {
        logging_database::InfractionType::Mute
    } else if lower_msg.contains("warn")
        || lower_msg.contains("barn")
        || lower_msg.contains("marm")
        || lower_msg.contains("warm")
        || lower_msg.contains("worm")
    {
        logging_database::InfractionType::Warn
    } else {
        logging_database::InfractionType::Unknown
    }
}

pub async fn parse_messages(
    logging_database: Arc<LoggingDB>,
    log_type: logging_database::LogType,
    messages: Vec<Message>,
) -> Result<(), BoxedError> {
    // Process all messages in parallel instead of chunks
    let futures: Vec<_> = messages
        .iter()
        .filter(|message| {
            !message
                .content
                .lines()
                .next()
                .is_some_and(|line| line.contains("probation"))
        })
        .map(|message| {
            let content = if let Some(ref_msg) = &message.referenced_message {
                ref_msg.content.clone()
            } else {
                message.content.clone()
            };

            let message = message.clone();

            async move {
                // Run Roblox and Discord ID lookups concurrently
                let (roblox_user_ids, discord_user_ids) = tokio::join!(
                    get_roblox_ids(&content),
                    get_discord_ids(&content)
                );

                Ok::<_, BoxedError>(Log {
                    log_type,
                    infraction_type: get_infraction_type(&content),
                    roblox_user_ids,
                    discord_user_ids,
                    reason: get_reason(&content),
                    message_id: message
                        .referenced_message
                        .map_or(message.id.get(), |ref_msg| ref_msg.id.get()),
                    channel_id: message.channel_id.get(),
                })
            }
        })
        .collect();

    // Process all messages concurrently
    let results: Vec<Log> = join_all(futures).await.into_iter().flatten().collect();

    // Only acquire the database lock once for all logs
    if !results.is_empty() {
        for result in results {
            logging_database.save_single(result).await?;
        }
    }

    Ok(())
}



fn extract_moderated_users(content: &str) -> Vec<u64> {
    let mut result = Vec::new();
    let mut current_number = String::new();
    
    // Skip first line as per requirement
    let content_without_first_line = content.lines().skip(1).collect::<Vec<_>>().join("\n");
    
    // Replace common formatting characters
    let cleaned = content_without_first_line
        .replace(['<', '>', '@', '[', ']'], "");
    
    let mut chars = cleaned.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            current_number.push(c);
            
            // Look ahead to see if we should stop collecting digits
            if let Some(&next_char) = chars.peek() {
                if !next_char.is_ascii_digit() {
                    // Only process numbers that are 16 digits or longer
                    if current_number.len() >= 16 {
                        if let Ok(num) = current_number.parse::<u64>() {
                            result.push(num);
                        }
                    }
                    current_number.clear();
                }
            }
        } else {
            // We hit a non-digit character
            if current_number.len() >= 16 {
                if let Ok(num) = current_number.parse::<u64>() {
                    result.push(num);
                }
            }
            current_number.clear();
            
            // Skip continuous chars until we hit a delimiter
            if c.is_alphabetic() {
                while let Some(&next_char) = chars.peek() {
                    if next_char == ':' || next_char == ' ' || next_char == ',' {
                        break;
                    }
                    chars.next();
                }
            }
        }
    }
    
    // Handle case where number is at the end of string
    if current_number.len() >= 16 {
        if let Ok(num) = current_number.parse::<u64>() {
            result.push(num);
        }
    }
    
    // Remove duplicates while preserving order
    result.sort_unstable();
    result.dedup();
    
    result
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_reason() {
        // Single line tests
        assert_eq!(get_reason("123456 This is the reason"), "This is the reason");
        assert_eq!(get_reason("1234567 Longer number reason"), "Longer number reason");
        assert_eq!(get_reason("12345 [Too short] 123456 [Valid reason]"), "Valid reason");
        
        // Multiline tests
        assert_eq!(
            get_reason("First line\n123456 Second line reason"),
            "Second line reason"
        );
        assert_eq!(
            get_reason("12345\n123456\nThis is the reason"),
            "This is the reason"
        );
        assert_eq!(
            get_reason("First: 123456\nSecond: 9876543\nThis is the final reason"),
            "This is the final reason"
        );
        assert_eq!(
            get_reason("Line1: 123456 First reason\nLine2: 9876543 Final reason"),
            "Final reason"
        );
        
        // Edge cases
        assert_eq!(get_reason("No numbers here"), "reason not found");
        assert_eq!(get_reason("12345 Too short"), "reason not found");
        assert_eq!(
            get_reason("123456789\n[Complex reason]\nwith brackets"),
            "Complex reason\nwith brackets"
        );
    }

    #[test]
    fn test_mute_format() {
        let input = "[mute]\n[465570298121682944]\n[earrape in vc]\n[580469129757196314 - dimensions06\n397817320246083586 - Superma7578 \nare witnesses\n+ i heard it]";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![465570298121682944]);
    }

    #[test]
    fn test_warn_format() {
        let input = "[warn]\n[590929310559502356\n346732032883294208\n780972325351718912]\n[harassment in vc\nhttps://easyupload.io/y51pt8\npassword is PROOF\n802585763837116446 - DoctorDonner is also a witness/victim]";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![346732032883294208, 590929310559502356, 780972325351718912]);
    }

    #[test]
    fn test_ban_format() {
        let input = "ban\n<@761626028308955186> 761626028308955186 cre_ato0r:561210051\n<@1130299525337202783> 1130299525337202783 cre_ato0r:561210051\nexploiting ingame\nhttps://discord.com/channels/570684122519830540/589530125360562177/1144800323651768330";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![761626028308955186, 1130299525337202783]);
    }

    #[test]
    fn test_warn_with_comma() {
        let input = "warn\n<@298729026535817218>, 298729026535817218\nfiltered slur - discrimination";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![298729026535817218]);
    }

    #[test]
    fn test_simple_warn() {
        let input = "Warn\n<@1057992157337767946> 1057992157337767946\nSuggestive";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![1057992157337767946]);
    }

    #[test]
    fn test_complex_ban_format() {
        let input = "[Ban]\n[<@1084205899922559077>:1084205899922559077:Not verified]  \n[<@922258742265913404>:922258742265913404:Stanleymia_36:1250835785]\n[<@990209922937524244>:990209922937524244:Brigi9988:469401376]\n[<@676748086860841019>:676748086860841019:bugaev_poshliy9let:1410377689]\n[<@777090962375704597>:777090962375704597:AleksanderAlex:380011602]\n[Affiliated with an exploiting server]";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![676748086860841019, 777090962375704597, 922258742265913404, 990209922937524244, 1084205899922559077]);
    }

    #[test]
    fn test_warn_with_witnesses() {
        let input = "warn\n<@1202366962005725227> 1202366962005725227 \nbypass in vc\nwitness <@767212267266899980> <@512350910433853451>";
        let result = extract_moderated_users(input);
        assert_eq!(result, vec![1202366962005725227]);
    }
}
