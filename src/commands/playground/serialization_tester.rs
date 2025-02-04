use serenity::all::GetMessages;
use crate::main_modules::helper;
use super::{Context, Error, serenity, FromStr};

#[poise::command(slash_command, prefix_command)]
/// This is to test serialization of messages in an ingame-logging environment.
pub async fn gamenight_helper(ctx: Context<'_>) -> Result<(), Error> {
    let msg = ctx.say("Getting latest messages and doing some calculations...").await?;
    let channel_id = ctx.channel_id();

    msg.delete(ctx).await?;
    Ok(())
}