#![allow(nonstandard_style)]
use regex::Regex;
use reqwest::header::HeaderValue;
use reqwest::Client;
use serde_json::Value;
use serenity::all::{CreateEmbed, CreateEmbedFooter};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::Data;
use unicode_segmentation::UnicodeSegmentation;

use super::{UserId, CONFIG};
use std::fmt::Write;

pub async fn discord_id_to_roblox_id(reqwest_client: &Client, discord_id: UserId) -> Result<String, String> {
    let quote_regex = Regex::new("/\"/gi").expect("regex err");
    let bloxlink_api_key: HeaderValue = CONFIG.main.bloxlink_api_key.parse::<HeaderValue>().expect("err");

    let url = format!("https://api.blox.link/v4/public/discord-to-roblox/{}", discord_id);
    let response = reqwest_client.get(url)
        .header("Authorization", bloxlink_api_key)
        .send()
        .await.expect("??");
    if response.status() != reqwest::StatusCode::OK {
        Err(format!("Something went wrong attempting to get Bloxlink data for user `{}`. They might not be verified with Bloxlink.", discord_id))
    } else {
        let serialized_json: Value = serde_json::from_str(response.text().await.expect("err").as_str()).expect("err");
        Ok(quote_regex.replace(serialized_json["robloxID"].as_str().unwrap(), "").to_string())
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
    let duration_list = duration_string.split(' ').map(str::to_string).collect::<Vec<String>>();
    let mut unix_total = 0;
    let mut final_string = String::new();
    if duration_list.is_empty() {return Err(format!("Something went wrong parsing duration string `{}`.", duration_string));} else {
        for duration in duration_list.clone() {
            let chars = duration.chars();
            let amount = match chars.clone().filter(|x| x.is_ascii_digit()).collect::<String>().parse::<u64>() {
                Ok(amount) => amount,
                Err(_) => {
                    return Err(format!("Something went wrong parsing duration string `{}`.", duration_string));
                },
            };
            let identifier = chars.last().expect("err");
            if !date_map.contains_key(identifier.to_string().as_str()) {
                return Err(format!("Something went wrong parsing duration string `{}`.", duration_string));
            }
            let mut name = date_map[&identifier.to_string().as_str()].1.to_string();
            if amount > 1 {name = format!("{} {}s, ", amount, name)} else {name = format!("{} {}, ", amount, name)}
            if duration_list.ends_with(&[duration.clone()]) {name.pop();name.pop();}
            if duration_list.ends_with(&[duration.clone()]) && !duration_list.starts_with(&[duration.clone()]) {name = format!("and {}", name);}
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
    let mut date_units = [(31556952, "Year"),
        (2629743, "Month"),
        (604800, "Week"),
        (86400, "Day"),
        (3600, "Hour"),
        (60, "Minute"),
        (1, "Second")];

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

pub async fn badge_data(reqwest_client: &Client, roblox_id: String, badge_iterations: i64) -> Result<(i64, f64, String), String> {
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
                        if cursor.is_empty() { String::new() } else { format!("&cursor={}", cursor) }
                    );

                    let response = reqwest_client.get(&url)
                        .send()
                        .await
                        .map_err(|e| format!("Request failed: {}", e))?;

                    if !response.status().is_success() {
                        return Err(format!("Request failed with status: {}", response.status()));
                    }

                    let text = response.text().await
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
                },
                Ok(_) => {}, // No more pages
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
        awarders_vec.iter().fold(String::new(), |mut acc, (id, count)| {
            let _ = write!(acc, "\n - {}: {}", id, count);
            acc
        })
    };

    Ok((badge_count, win_rate, awarders_string))
}

pub async fn roblox_friend_count(reqwest_client: &Client, roblox_id: &str) -> Result<usize, String> {
    let url = format!("https://friends.roblox.com/v1/users/{}/friends", roblox_id);
    let response = reqwest_client.get(&url).send().await.unwrap();
    let response_text = response.text().await.unwrap();
    
    let parsed_json: Value = serde_json::from_str(&response_text).unwrap();
    
    Ok(parsed_json["data"].as_array()
        .ok_or_else(|| "Data is not an array".to_string())?
        .len())
}

pub async fn roblox_group_count(reqwest_client: &Client, roblox_id: &str) -> Result<usize, String> {
    let url = format!("https://groups.roblox.com/v2/users/{}/groups/roles?includeLocked=true", roblox_id);
    let response = reqwest_client.get(&url).send().await.unwrap();
    let response_text = response.text().await.unwrap();
    
    let parsed_json: Value = serde_json::from_str(&response_text).unwrap();
    
    Ok(parsed_json["data"].as_array()
        .ok_or_else(|| "Data is not an array".to_string())?
        .len())
}

pub async fn merge_types(reqwest_client: &Client, rbx_client: &roboat::Client, users: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut roblox_ids: Vec<String> = Vec::new();
    let mut errors_vector: Vec<String> = Vec::new();

    for user in users {
        if user.len() >= 17 && user.chars().all(|c| c.is_ascii_digit()) {
            let discord_id = match UserId::from_str(user.as_str()) {Ok(id) => id, Err(err) => {
                errors_vector.push(format!("Couldn't find turn discord id string into actual discord id for {}, details:\n{}", user, err));
                continue
            }};
            let roblox_id_str = match self::discord_id_to_roblox_id(reqwest_client, discord_id).await {Ok(id) => id, Err(err) => {
                errors_vector.push(format!("Couldn't find turn discord id into roblox id for {}, details:\n{}", user, err));
                continue
            }};
            roblox_ids.push(roblox_id_str)
        } else if user.len() < 17 && user.chars().all(|c| c.is_ascii_digit()) {
            roblox_ids.push(user)
        } else if !user.chars().all(|c| c.is_ascii_digit()) {
            let user_search = match rbx_client.username_user_details(vec![user.clone()], false).await {Ok(id) => id, Err(err) => {
                errors_vector.push(format!("Couldn't find user details for {}, details:\n{}", user, err));
                continue
            }};
            for details in user_search {
                roblox_ids.push(details.id.to_string())
            }
        }
    }
    (roblox_ids, errors_vector)
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
    CreateEmbed::new().color(framework_data.bot_color).footer(CreateEmbedFooter::new("Made by RabbyDevs, with 🦀 and ❤️.").icon_url(framework_data.bot_avatar.clone()))
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
            processed_content = processed_content.replace(whole_match.as_str(), &" ".repeat(whole_match.len()));
        }
    }

    let unicode_emojis: Vec<String> = processed_content
        .graphemes(true) 
        .filter(|&g| emojis::get(g).is_some())
        .map(|g| g.to_string()) 
        .collect();

    (unicode_emojis, custom_emojis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emoji_extraction() {
        let message = "Hello 👋 :wave: world! <:custom:123456> 🌍 :earth: <:other:789012>";
        let (unicode, custom) = extract_emojis(message);
        
        assert_eq!(unicode, vec!["👋".to_string(), "🌍".to_string()]);
        assert_eq!(custom, vec![
            ("custom".to_string(), 123456),
            ("other".to_string(), 789012)
        ]);
    }

    #[test]
    fn test_with_only_unicode() {
        let message = "Hello 👋 🌍 world!";
        let (unicode, custom) = extract_emojis(message);
        
        assert_eq!(unicode, vec!["👋".to_string(), "🌍".to_string()]);
        assert_eq!(custom, Vec::<(String, u64)>::new());
    }
}