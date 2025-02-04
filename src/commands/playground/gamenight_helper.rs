use serenity::all::{GetMessages, ReactionType};
use serenity::model::id::EmojiId;
use crate::main_modules::helper;
use super::{Context, Error, serenity, FromStr};

#[poise::command(slash_command, prefix_command)]
/// All this does is literally just react with all the emojis in the last message that had emojis.
pub async fn gamenight_helper(ctx: Context<'_>) -> Result<(), Error> {
    let msg = ctx.say("Getting latest message and doing some calculations...").await?;
    let channel_id = ctx.channel_id();
    
    let last_10_messages = channel_id
        .messages(ctx.http(), GetMessages::new().limit(10))
        .await
        .map_err(Error::from)?;

    for message in last_10_messages.iter().rev() {
        let (unicode_emojis, custom_emojis) = helper::extract_emojis(&message.content);
        
        if unicode_emojis.is_empty() && custom_emojis.is_empty() {
            println!("continuing");
            continue;
        }

        // Handle unicode emojis
        for emoji in unicode_emojis {
            if let Ok(reaction_type) = ReactionType::from_str(&emoji) {
                if let Err(e) = message.react(ctx.http(), reaction_type).await {
                    eprintln!("Failed to react with unicode emoji: {}", e);
                }
            }
        }

        // Handle custom emojis
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