use std::{fs, io::Write, path::Path, time::Duration};
use futures::StreamExt;
use serde::{Serialize, Deserialize};
use serenity::all::{ChannelId, Context, Message};
use sled::Db;

use super::CONFIG;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
pub struct PolicyEntry {
    pub content: String,
    pub order: u64,
}

#[derive(Debug, Clone)]
struct TocEntry {
    level: usize,
    title: String,
    link: String,
    children: Vec<TocEntry>,
}

#[derive(Clone)]
pub struct PolicySystem {
    db: Arc<Db>,
}



impl PolicySystem {
    pub fn init(db_path: &str) -> sled::Result<Self> {
        let db = Arc::new(sled::open(db_path)?);
        let system = PolicySystem {
            db: Arc::clone(&db)
        };

        Ok(system)
    }

    pub fn edit(&self, internal_name: &str, content: String, order: u64) -> sled::Result<()> {
        let entry = PolicyEntry { content, order };
        let serialized = bincode::serialize(&entry).map_err(|_| sled::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "Serialization error")))?;
        self.db.insert(internal_name, serialized)?;
        Ok(())
    }

    pub fn remove(&self, internal_name: &str) -> sled::Result<()> {
        self.db.remove(internal_name)?;
        Ok(())
    }

    pub fn list_policies(&self) -> sled::Result<Vec<(String, PolicyEntry)>> {
        let mut policies = Vec::new();
        
        for result in self.db.iter() {
            let (key, value) = result?;
            let key_str = String::from_utf8(key.to_vec()).map_err(|_| sled::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "UTF-8 Error")))?;
            let entry: PolicyEntry = bincode::deserialize(&value)
                .map_err(|_| sled::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "Deserialization error")))?;
            
            policies.push((key_str, entry));
        }
        
        policies.sort_by_key(|(_, entry)| entry.order);

        Ok(policies)
    }

    pub fn list_policies_internal_names(&self) -> sled::Result<Vec<(String, PolicyEntry)>> {
        let mut policies = Vec::new();
        
        for result in self.db.iter() {
            let (key, value) = result?;
            let key_str = String::from_utf8(key.to_vec()).map_err(|_| sled::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "UTF-8 Error")))?;
            let entry: PolicyEntry = bincode::deserialize(&value)
                .map_err(|_| sled::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "Deserialization error")))?;
            
            policies.push((key_str, entry));
        }

        policies.sort_by_key(|(_, entry)| entry.order);

        Ok(policies)
    }

    pub async fn update_policy(&self, ctx: &Context) -> sled::Result<()> {
        let policies = self.list_policies()?;
    
        let mut file_contents = String::new();
        for (_, policy) in policies.iter() {
            file_contents.push_str(&format!(
                "{}\n** **\n",
                policy.content
            ));
        }
    
        let previous_file_path = Path::new("policy.txt");
        let current_file_path = Path::new("current_policy.txt");
    
        if previous_file_path.exists() {
            let previous_content = fs::read_to_string(previous_file_path).unwrap_or_default();
            if previous_content != file_contents {
                let changes_channel_id = CONFIG.modules.policy.policy_changes_channel_id.parse::<u64>().unwrap();
                let changes_channel = ctx.http.get_channel(changes_channel_id.into()).await.unwrap();
    
                let diff = diff_policies(&previous_content, &file_contents);
                send_code_blocks(ctx, &changes_channel.id(), "Policy updates detected:", &diff).await;
            }
        }
    
        let mut file = fs::File::create(current_file_path)?;
        file.write_all(file_contents.as_bytes())?;
    
        let policy_channel_id = CONFIG.modules.policy.policy_channel_id.parse::<u64>().unwrap();
        let policy_channel = ctx.http.get_channel(policy_channel_id.into()).await.unwrap();
    
        let policy_actual_id = ChannelId::new(policy_channel_id);
        let mut message_stream = policy_actual_id.messages_iter(ctx).boxed();
        let mut messages_to_delete = Vec::new();
                        
        while let Some(message_result) = message_stream.next().await {
            let message = message_result.unwrap();
            messages_to_delete.push(message.id);
        }
                        
        while !messages_to_delete.is_empty() {
            let to_delete = messages_to_delete.split_off(messages_to_delete.len().saturating_sub(100));
            policy_actual_id.delete_messages(ctx, to_delete).await.unwrap();
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    
        let mut all_headings = Vec::new();
    
        for (_, policy) in policies.iter() {
            // Split content into sections based on headings
            let mut sections = Vec::new();
            let mut current_section = String::new();
            let mut lines = policy.content.lines().peekable();
            
            while let Some(line) = lines.next() {
                if line.starts_with('#') && !current_section.is_empty() {
                    sections.push(current_section.clone());
                    current_section.clear();
                }
                current_section.push_str(line);
                current_section.push('\n');
                
                // If this is the last line or next line starts a new section
                if (lines.peek().is_none() || lines.peek().map_or(false, |next| next.starts_with('#'))) && !current_section.is_empty() {
                    sections.push(current_section.clone());
                    current_section.clear();
                }
            }
    
            // Send each section separately and collect their links
            for section in sections {
                let messages = send_long_message(ctx, &policy_channel.id(), &format!("{}\n** **", section)).await;
                let message_link = messages.first().unwrap().link(); // Use first message link for the section
                
                // Extract headings from this section with the correct message link
                let section_headings = extract_headings(&section, &message_link);
                all_headings.extend(section_headings);
            }
        }
    
        // Sort headings by their numeric prefix
        all_headings.sort_by(|a, b| {
            let a_num = extract_number(&a.title);
            let b_num = extract_number(&b.title);
            a_num.cmp(&b_num)
        });
    
        let toc_tree = build_toc_hierarchy(all_headings);
        let toc_content = format_toc(&toc_tree);
    
        send_long_message(ctx, &policy_channel.id(), &format!("# Table of Contents\n{}", toc_content)).await;
    
        fs::rename(current_file_path, previous_file_path)?;
    
        Ok(())
    }

    pub fn clear_all(&self) -> sled::Result<()> {
        let policies = self.list_policies_internal_names()?;
        for (internal_name, _) in policies {
            self.db.remove(internal_name)?;
        }
        Ok(())
    }
}

async fn send_code_blocks(ctx: &Context, channel_id: &ChannelId, prefix: &str, content: &str) -> Vec<Message> {
    let mut messages = Vec::new();
    let max_content_length = 1990;
    
    if !prefix.is_empty() {
        let message = channel_id.say(ctx, prefix).await.unwrap();
        messages.push(message);
    }

    let mut current_block = String::new();
    
    for line in content.lines() {
        if current_block.len() + line.len() + 8 > max_content_length && !current_block.is_empty() {
            let formatted_block = format!("```diff\n{}```", current_block);
            let message = channel_id.say(ctx, formatted_block).await.unwrap();
            messages.push(message);
            current_block.clear();
        }
        current_block.push_str(line);
        current_block.push('\n');
    }

    if !current_block.is_empty() {
        let formatted_block = format!("```diff\n{}```", current_block);
        let message = channel_id.say(ctx, formatted_block).await.unwrap();
        messages.push(message);
    }

    messages
}

async fn send_long_message(ctx: &Context, channel_id: &ChannelId, content: &str) -> Vec<Message> {
    let mut messages = Vec::new();
    let mut buffer = String::new();
    
    for line in content.lines() {
        if buffer.len() + line.len() + 1 > 2000 {
            let message = channel_id.say(ctx, buffer.clone()).await.unwrap();
            messages.push(message);
            buffer.clear();
        }
        
        buffer.push_str(line);
        buffer.push('\n');
    }

    if !buffer.is_empty() {
        let message = channel_id.say(ctx, buffer).await.unwrap();
        messages.push(message);
    }

    messages
}

fn build_toc_hierarchy(headings: Vec<TocEntry>) -> Vec<TocEntry> {
    let mut result = Vec::new();
    let mut stack: Vec<TocEntry> = Vec::new();
    
    for heading in headings {
        while !stack.is_empty() && stack.last().unwrap().level >= heading.level {
            stack.pop();
        }
        
        let new_entry = TocEntry {
            level: heading.level,
            title: heading.title.clone(),
            link: heading.link.clone(),
            children: Vec::new(),
        };
        
        if let Some(parent) = stack.last_mut() {
            parent.children.push(new_entry.clone());
        } else {
            result.push(new_entry.clone());
        }
        
        if heading.level > 1 {
            stack.push(new_entry);
        }
    }
    
    result
}

fn format_toc(entries: &[TocEntry]) -> String {
    fn format_entry(entry: &TocEntry, parent_number: &str, index: usize, output: &mut String) {
        let indent = "  ".repeat(entry.level.saturating_sub(1));
        
        // Extract existing number from title if it exists
        let title_number = extract_number(&entry.title);
        
        let entry_number = if entry.level == 1 {
            // For level 1, use the number from the title
            title_number
                .map(|n| n.to_string())
                .unwrap_or_else(|| (index + 1).to_string())
        } else {
            // For level 2+, construct the hierarchical number
            format!("{}.{}", parent_number, index + 1)
        };

        // Clean the title by removing the number prefix if it exists
        let clean_title = entry.title
            .split_whitespace()
            .skip(if title_number.is_some() { 1 } else { 0 })
            .collect::<Vec<_>>()
            .join(" ");
        
        // Format the TOC entry
        output.push_str(&format!("{}{} [{}]({})\n",
            indent,
            if entry.level == 1 { format!("{}.", entry_number) } else { entry_number.clone() },
            clean_title,
            entry.link
        ));
        
        // Process children with current entry number as parent
        for (i, child) in entry.children.iter().enumerate() {
            format_entry(child, &entry_number, i, output);
        }
    }
    
    let mut output = String::new();
    for (i, entry) in entries.iter().enumerate() {
        format_entry(entry, "", i, &mut output);
    }
    output
}

// Modified extract_headings to use clean titles
fn extract_headings(content: &str, message_link: &str) -> Vec<TocEntry> {
    let mut headings = Vec::new();
    
    for line in content.lines() {
        if line.starts_with('#') {
            let level = line.chars().take_while(|&c| c == '#').count();
            let title = line.trim_start_matches('#').trim().to_string();
            
            if !title.is_empty() {
                headings.push(TocEntry {
                    level,
                    title,
                    link: message_link.to_string(),
                    children: Vec::new(),
                });
            }
        }
    }
    
    headings
}
// Helper function to extract numbers from section titles
fn extract_number(title: &str) -> Option<u32> {
    title.split_whitespace()
        .next()?
        .parse::<u32>()
        .ok()
}

fn diff_policies(previous: &str, current: &str) -> String {
    use similar::{TextDiff, ChangeTag};

    let mut changes = String::new();
    let diff = TextDiff::from_lines(previous, current);

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => changes.push_str(&format!("- {}", change)),
            ChangeTag::Insert => changes.push_str(&format!("+ {}", change)),
            _ => {}
        }
    }

    changes
}