use poise::ChoiceParameter;
use serenity::all::CreateMessage;

use super::{Context, Error, helper};

#[derive(Debug, poise::ChoiceParameter)]
pub enum RobloxInfTypes {
    #[name = "Game Ban"]
    Ban,
    #[name = "Temporary Game Ban"]
    TempBan,
    #[name = "Game Server Ban"]
    ServerBan,
    Kick,
    Warn,
}

#[poise::command(slash_command, prefix_command)]
/// Creates a log for a ingame infraction. **Do not input Discord IDs as a test, please.**
#[warn(clippy::too_many_arguments)]
pub async fn robloxlog(
    ctx: Context<'_>,
    #[description = "Users for the command, accepts Discord ids, ROBLOX users and ROBLOX ids."]
    users: String,
    #[description = "Type of infraction."]
    #[rename = "type"]
    infraction_type: RobloxInfTypes,
    #[description = "Reason for infraction, split with |."] reason: String,
    #[description = "Note for the infraction, split with |."] note: Option<String>,
    #[description = "Multimessage mode allows creation of multiple logs from 1 command."]
    multimessage: Option<bool>,
) -> Result<(), Error> {
    ctx.reply("Making logs...").await?;
    let multimessage = multimessage.unwrap_or_default();
    let users: Vec<String> = users
        .split(' ')
        .map(str::to_string)
        .collect::<Vec<String>>();
    let roblox_conversion_errors;
    let roblox_ids;
    (roblox_ids, roblox_conversion_errors) =
        helper::merge_types(&ctx.data().reqwest_client, &ctx.data().rbx_client, users).await;
    if roblox_ids.is_empty() {
        ctx.channel_id().say(ctx, "Command failed; every user was converted and no valid users were found, meaning you might have inputted the users incorrectly...").await?;
        return Ok(());
    }

    for error in roblox_conversion_errors {
        ctx.channel_id().say(ctx, error).await?;
    }

    let reasons = reason
        .split('|')
        .map(str::to_string)
        .collect::<Vec<String>>();
    let notes = note
        .unwrap_or_default()
        .split('|')
        .map(str::to_string)
        .collect::<Vec<String>>();

    let mut users_string = String::new();
    let mut user_string_vec: Vec<String> = Vec::new();
    for id in roblox_ids {
        if id.is_empty() {
            continue;
        }
        let user_details = ctx
            .data()
            .rbx_client
            .user_details(id.parse::<u64>().expect("err"))
            .await?;
        let value = format!("[{}:{}]\n", user_details.username, user_details.id);
        if !multimessage {
            users_string.push_str(value.as_str())
        } else {
            user_string_vec.push(value)
        };
        let serenity_ctx = ctx.serenity_context().clone();
        let channel_id = ctx.channel_id();
        let reqwest_client = ctx.data().reqwest_client.clone();
        let data = ctx.data().clone();
        tokio::spawn(async move {
            let embed = helper::new_embed_from_template(&data)
                .await
                .title("Additional Information")
                .color(data.bot_color);
            if let Ok(associated_ids) = helper::roblox_id_to_discord_ids(&reqwest_client, id).await
                && let Err(err) = channel_id
                    .send_message(
                        &serenity_ctx.http,
                        CreateMessage::new().add_embed(
                            embed.field(
                                "Associated Discord IDs",
                                associated_ids
                                    .iter()
                                    .map(|s| format!("<@{s}>:{s}"))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                                true,
                            ),
                        ),
                    )
                    .await
            {
                eprintln!("Failed to send message: {err:?}");
            }
        });
    }

    let type_string = format!("[{}]\n", infraction_type.name());
    if !multimessage {
        let reason_string = format!("[{}]", reasons[0]);
        let note_string = if !notes[0].is_empty() {
            format!("\nNote: {}", notes[0])
        } else {
            String::new()
        };
        let response = format!(
            "{}{}{}{}",
            type_string, users_string, reason_string, note_string
        );
        ctx.say(response).await?;
    } else {
        let mut reason_number = 0;
        let mut note_number = 0;
        for user_string in user_string_vec {
            let reason_string = format!("[{}]", reasons[reason_number]);
            let note_string = if !notes[note_number].is_empty() {
                format!("\nNote: {}", notes[note_number])
            } else {
                String::new()
            };
            let response = format!(
                "{}{}{}{}",
                type_string, user_string, reason_string, note_string
            );
            ctx.say(response).await?;
            if reasons.get(reason_number + 1).is_some() {
                reason_number += 1
            }
            if notes.get(note_number + 1).is_some() {
                note_number += 1
            };
        }
    }
    Ok(())
}
