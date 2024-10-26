use poise::CreateReply;
use ::serenity::all::CreateMessage;

use super::{Context, Error, helper, UserId, Mentionable, serenity, FromStr};

#[poise::command(slash_command, prefix_command, 
    subcommands("add", "delete", "toggle_pause", "list"), 
    subcommand_required)]
pub async fn timed_role(
    _: Context<'_>,
) -> Result<(), Error> {
    Ok(())
}

#[poise::command(slash_command)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Users for the command, only accepts Discord ids."] users: String,
    #[description = "Type of infraction."] role: serenity::model::guild::Role,
    #[description = "Duration of the probation (e.g., '1h', '2d', '1w')."] duration: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let purified_users = ctx.data().number_regex.replace_all(users.as_str(), "");
    if purified_users.is_empty() {
        ctx.say("Command failed; no users inputted, or users improperly inputted.").await?;
        return Ok(());
    }

    let users: Vec<UserId> = purified_users
        .split_whitespace()
        .filter_map(|id| UserId::from_str(id).ok())
        .collect();

    let guild_id = ctx.guild_id().ok_or("Command must be used in a guild")?;

    let (current_time, unix_timestamp, timestamp_string) = match helper::duration_conversion(duration).await {
        Ok(result) => result,
        Err(err) => {
            ctx.say(format!("Error processing duration: {}", err)).await?;
            return Ok(());
        }
    };

    let duration_secs = unix_timestamp - current_time;

    for user_id in users {
        let timer_id = match ctx.data().timer_system.add_timer(user_id.to_string(), role.id.to_string(), duration_secs, false, None).await {
            Ok(id) => id,
            Err(err) => {
                ctx.say(format!("Failed to add timer for user {}: {}", user_id, err)).await?;
                continue;
            }
        };

        if (ctx.http().add_member_role(guild_id, user_id, role.id, None).await).is_err() {
            ctx.say(format!("Timer added but paused: {}, but failed to add role to user. (Are they in the server?)", user_id)).await?;
            let mut errored = false;
            ctx.data().timer_system.toggle_timer(&user_id.to_string(), &timer_id).await.unwrap_or_else(|_| {
                errored = true;
                None
            });
            if errored {
                ctx.say(format!("Failed to pause timer for user {}.", user_id)).await.unwrap();
            }
            continue;
        }
        ctx.say(format!("Role timer {} added for user {} for {}", role.id, user_id.mention(), timestamp_string)).await?;
    }
    Ok(())
}

#[poise::command(slash_command)]
pub async fn delete(
    ctx: Context<'_>,
    #[description = "Users for the command, only accepts Discord ids."] user: UserId,
    #[description = "Users for the command, only accepts Discord ids."] timer_id: String
) -> Result<(), Error> {
    let msg = ctx.say("Deleting timer...").await?;

    match ctx.data().timer_system.delete_timer(&user.to_string(), &timer_id).await {
        Ok(_) => {
            msg.edit(ctx, CreateReply::default().content("Timer deleted successfully.")).await?;
            Ok(())
        },
        Err(_) => Err(Error::from("Uh oh something went wrong.")),
    }
}

#[poise::command(slash_command)]
pub async fn toggle_pause(
    ctx: Context<'_>,
    #[description = "Users for the command, only accepts Discord ids."] user: UserId,
    #[description = "Users for the command, only accepts Discord ids."] timer_id: String
) -> Result<(), Error> {
    let msg = ctx.say("Toggling pause status...").await?;

    match ctx.data().timer_system.toggle_timer(&user.to_string(), &timer_id).await {
        Ok(_) => {
            msg.edit(ctx, CreateReply::default().content("Pause toggled successfully.")).await?;
            Ok(())
        },
        Err(_) => Err(Error::from("Uh oh something went wrong.")),
    }
}

#[poise::command(slash_command)]
pub async fn list(
    ctx: Context<'_>,
    #[description = "Users for the command, only accepts Discord ids."] user: UserId
) -> Result<(), Error> {
    ctx.say("Sending timers set under users.").await?;

    let timers = ctx.data().timer_system.list_user_timers(&user.to_string()).await;
    if timers.is_empty() {
        ctx.say("No timers set under user.").await?;
        return Ok(())
    }
    for timer in timers {
        let embed= helper::new_embed_from_template(ctx.data()).await
            .title(format!("Timer ID - {}", timer.timer_id))
            .field("Role ID", timer.role_id, true)
            .field("End Timestamp", format!("<t:{}:D>", timer.end_timestamp), true)
            .field("Is Paused", format!("{}", timer.is_paused), true)
            .field("Paused Duration (only calculated afer unpause)", helper::format_duration(timer.paused_duration), true);
        ctx.channel_id().send_message(ctx.http(), CreateMessage::new().embed(embed)).await?;
    }
    Ok(())
}