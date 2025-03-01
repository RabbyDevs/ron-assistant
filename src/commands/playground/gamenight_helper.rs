use serenity::all::{GetMessages, ReactionType};
use serenity::model::id::EmojiId;
use crate::main_modules::helper;
use super::{Context, Error, serenity, FromStr};

#[poise::command(slash_command, prefix_command)]
/// All this does is literally just react with all the emojis in the last message that had emojis.
pub async fn gamenight_helper(
    ctx: Context<'_>,
    message_count: u64
) -> Result<(), Error> {
    let msg = ctx.say("Getting latest message and doing some calculations...").await?;
    let channel_id = ctx.channel_id();
    
    let last_messages: Vec<serenity::Message> = channel_id
        .messages(ctx.http(), GetMessages::new().limit(message_count.try_into().unwrap_or(5)))
        .await
        .map_err(Error::from)?;

    for message in last_messages.iter() {
        let (unicode_emojis, custom_emojis) = helper::extract_emojis(&message.content);
        
        if unicode_emojis.is_empty() && custom_emojis.is_empty() {
            continue;
        }

        for emoji in unicode_emojis {
            if let Ok(reaction_type) = ReactionType::from_str(&emoji) {
                if let Err(e) = message.react(ctx.http(), reaction_type).await {
                    eprintln!("Failed to react with unicode emoji: {}", e);
                }
            }
        }

        for (_, emoji_id) in custom_emojis {
            let reaction_type = ReactionType::Custom {
                animated: false,
                id: EmojiId::new(emoji_id),
                name: None,
            };
            
            if let Err(e) = message.react(ctx.http(), reaction_type).await {
                eprintln!("Failed to react with custom emoji: {}", e);
            }
        }
    }

    msg.delete(ctx).await?;
    Ok(())
}