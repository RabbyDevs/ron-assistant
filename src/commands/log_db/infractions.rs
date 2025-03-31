use serenity::all::{CreateEmbed, CreateEmbedAuthor, CreateMessage, UserId};

use crate::main_modules::helper;
use super::{Context, Error};

#[poise::command(slash_command, prefix_command)]
/// This is to test serialization of messages in an ingame-logging environment.
pub async fn inf(
    ctx: Context<'_>,
    #[description = "Users for the command, accepts Discord ids, ROBLOX users and ROBLOX ids."] users: String
) -> Result<(), Error> {
    ctx.say("Getting message(s) and doing some calculation(s)...").await?;
    let users: Vec<String> = users.split_whitespace().map(str::to_string).collect();

    let (roblox_users, discord_users, errors) = helper::split_types(&ctx.data().rbx_client, users).await;

    if !errors.is_empty() {
        ctx.channel_id().say(&ctx.http(), &errors.join("\n")).await?;
    }

    for user in roblox_users {        
        println!("{}", user);
        let logs = ctx.data().logging_db.get_by_id(user).await;
        println!("{:#?}", logs);
    }

    for user in discord_users {
        println!("{}", user);
        let logs = ctx.data().logging_db.get_by_id(user).await;
        println!("{:#?}", logs);

        // let mut embeds: Vec<CreateEmbed> = vec![];
        // let split_logs = logs.chunks(5);

        // let discord_user = ctx.http().get_user(UserId::new(user)).await?;
        
        // for chunk in split_logs {
        //     let mut embed = helper::new_embed_from_template(ctx.data()).await;
        //     embed = embed.author(CreateEmbedAuthor::new(format!("{}'s Moderation History", discord_user.name.clone())).icon_url(discord_user.avatar_url().unwrap()));
        //     for log in chunk {
        //         let log_msg = ctx.http().get_message(log.channel_id.into(), log.message_id.into()).await.unwrap();
        //         embed = embed.field(format!("[Log #{}]{} |", log.message_id, log_msg.link()), format!("{}", log.log_type), false)
        //     }
        //     embeds.push(embed);
        // }

        // for embed in embeds {
        //     ctx.channel_id().send_message(&ctx.http(), CreateMessage::new().add_embed(embed.clone())).await?;
        // }
    }
    Ok(())
}