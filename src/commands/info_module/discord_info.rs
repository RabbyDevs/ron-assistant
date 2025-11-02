use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ::serenity::all::{CreateMessage, RoleId};
use serenity::User;
use std::collections::HashMap;
use crate::main_modules::helper;

use super::{Context, Error, UserId, serenity, FromStr};

fn split_string(s: String, chunk_size: usize) -> Vec<String> {
    s.chars()
        .collect::<Vec<char>>()
        .chunks(chunk_size)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

#[poise::command(slash_command, prefix_command)]
/// Gets all possible information about the discord account.
pub async fn discordinfo(
    ctx: Context<'_>,
    #[description = "Discord user ids for the command."] users: String,
) -> Result<(), Error> {
    ctx.reply("Getting user info, please standby!").await?;
    let purified_users = ctx.data().number_regex.replace_all(users.as_str(), "");
    if purified_users.is_empty() {
        ctx.say("Command failed; no users inputted, or users improperly inputted.").await?;
        return Ok(());
    }
    let users = purified_users.split(' ');
    for snowflake in users {
        let userid: UserId = UserId::from_str(snowflake).expect("something went wrong.");
        let user: User = match userid.to_user(&ctx.http()).await {
            Ok(user) => user,
            Err(_) => {
                ctx.say(format!("An error occurred attempting to process user `{}`. Skipping user's log.", snowflake)).await?;
                continue;
            }
        };

        ctx.channel_id().say(&ctx.http(), "### User Id").await?;
        ctx.channel_id().say(&ctx.http(), format!("{}", user.id)).await?;
        ctx.channel_id().say(&ctx.http(), "### User Mention").await?;
        ctx.channel_id().say(&ctx.http(), format!(r"<\@{}>", user.id)).await?;

        let created_at_timestamp = user.created_at().unix_timestamp();
        let account_age = SystemTime::now().duration_since(UNIX_EPOCH)? - Duration::from_secs(created_at_timestamp as u64);
        let new_account_message = if account_age < Duration::from_secs(60 * 24 * 60 * 60) {
            "**Account is new, below 60 days old.**"
        } else {
            ""
        };

        let avatar_url = match user.avatar_url() {
            Some(url) => url,
            None => "No URL/User has a default avatar.".to_string()
        };
        let banner_url = match user.banner_url() {
            Some(url) => url,
            None => "No banner.".to_string()
        };

        let global_name = match user.global_name {
            Some(global_name) => global_name,
            None => "No nickname set.".to_string()
        };

        let mut first_embed = helper::new_embed_from_template(ctx.data()).await
            .title("Extra User Information")
            .field("Username", user.name.to_string(), true)
            .field("Global Name", global_name.to_string(), true)
            .field("User Creation Date", format!("<t:{}:D>\n{}", created_at_timestamp, new_account_message), true)
            .field("Avatar URL", avatar_url.to_string(), true)
            .field("Banner URL", banner_url.to_string(), true);
        let mut embeds = vec![];

        if let Some(guild_id) = ctx.guild_id() {
            if let Ok(member) = guild_id.member(&ctx.http(), userid).await {
                let nickname = match member.clone().nick {
                    Some(nickname) => nickname,
                    None => "No nickname set.".to_string()
                };
        
                let mut role_permissions: HashMap<RoleId, Vec<&'static str>> = HashMap::new();
                
                if let Ok(guild) = guild_id.to_partial_guild(&ctx.http()).await {
                    for (role_id, role) in &guild.roles {
                        let perm_names: Vec<&'static str> = role.permissions
                            .iter_names()
                            .map(|(name, _)| name)
                            .collect();
                        role_permissions.insert(*role_id, perm_names);
                    }
                }
        
                let role_string = member.roles
                    .iter()
                    .map(|roleid| format!("<@&{}>", roleid))
                    .collect::<Vec<String>>()
                    .join(" ");

                let role_chunks = split_string(role_string, 1000);
                let mut role_embeds = vec![];
                for (i, chunk) in role_chunks.iter().enumerate() {
                    let role_embed = helper::new_embed_from_template(ctx.data()).await
                        .title(format!("Guild Member Roles (Part {})", i + 1))
                        .description(chunk);
                    role_embeds.push(role_embed);
                }
                
                first_embed = first_embed.field("Member Nickname", nickname, true);
                embeds.push(first_embed);
                embeds.extend(role_embeds);

                if embeds.len() > 10 {
                    ctx.channel_id().say(&ctx.http(), "Warning: Too many embeds to send in one message. Some information may be truncated.").await?;
                    embeds.truncate(10);
                }
            }
        } else {
            first_embed = first_embed
                .field("Note", "This command was used outside of a guild context. Role and permission information is not available.", false);
            embeds.push(first_embed);
        }

        ctx.channel_id().send_message(&ctx.http(), CreateMessage::default().embeds(embeds)).await?;
    }
    Ok(())
}