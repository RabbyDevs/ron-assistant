use futures::future::join_all;
use serenity::all::MessageId;
use super::{Context, Error, video_convert};
use poise::serenity_prelude as serenity;
use std::sync::Arc;

#[poise::command(slash_command, prefix_command)]
/// Command for activating the video convertor on specific messages.
pub async fn convert_video(
    ctx: Context<'_>,
    #[description = "Message ids for command."] message_ids: String,
) -> Result<(), Error> {
    ctx.reply("Converting message videos, searching thru each channel, this may take a while!").await?;

    let message_ids: Vec<u64> = message_ids
        .split_whitespace()
        .filter_map(|id| id.parse().ok())
        .collect();

    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => {
            ctx.say("Use this command in a guild!").await?;
            return Ok(());
        }
    };

    let http = Arc::new(ctx.http());
    let channels = http.get_channels(guild_id).await?;

    let futures = message_ids.into_iter().map(|message_id| {
        let channels = channels.clone();
        let http = Arc::clone(&http);
        let ctx: poise::Context<'_, crate::Data, Box<dyn std::error::Error + Send + Sync>> = ctx;
        async move {
            process_message(channels, &http, ctx, MessageId::new(message_id)).await;
        }
    });

    join_all(futures).await;

    ctx.say("All videos have been processed!").await?;
    Ok(())
}

async fn process_message(
    channels: Vec<serenity::GuildChannel>,
    http: &serenity::Http,
    ctx: poise::Context<'_, crate::Data, Box<dyn std::error::Error + Send + Sync>>,
    message_id: MessageId,
) {
    let mut message = None;

    for channel in channels {
        if let Ok(found_message) = channel.message(&http, message_id).await {
            message = Some(found_message);
            break;
        }
    }

    let message = match message {
        Some(msg) => msg,
        None => {
            println!("Couldn't find message in guild, sorry!");
            return;
        }
    };

    let futures = message.attachments.iter().map(|attachment| {
        let message = message.clone();
        let attachment = attachment.clone();
        async move {
            video_convert(message, ctx.serenity_context().clone(), ctx.data().reqwest_client.clone(), attachment).await;
        }
    });

    join_all(futures).await;
}