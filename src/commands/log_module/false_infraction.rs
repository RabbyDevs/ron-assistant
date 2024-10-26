use poise::ChoiceParameter;
use serenity::all::Mentionable;

use super::{Context, Error, UserId, FromStr};

#[derive(Debug, poise::ChoiceParameter)]
pub enum FalseInfTypes {
    #[name = "Discord, Temporary Ban"]
    Ban,
    #[name = "Discord, Ban"]
    TempBan,
    #[name = "Discord, Kick"]
    Kick,
    #[name = "Discord, Mute"]
    Mute,
    #[name = "Discord, Warn"]
    Warn,
    #[name = "Game, Ban"]
    GameBan,
    #[name = "Game, Temporary Ban"]
    GameTempBan,
    #[name = "Game, Serverban"]
    GameServerBan,
    #[name = "Game, Kick"]
    GameKick,
    #[name = "Game, Warn"]
    GameWarn
}

async fn do_affected_id(rbx_client: &roboat::Client, user: &str) -> (String, Vec<String>) {
    let mut errors_vector = vec![];
    let mut response_edit = String::new();
    if user.len() >= 17 && user.chars().all(|c| c.is_ascii_digit()) {
        let discord_id = match UserId::from_str(user) {Ok(id) => id, Err(err) => {
            errors_vector.push(format!("Couldn't find turn discord id string into actual discord id for {}, details:\n{}", user, err));
            return (response_edit, errors_vector)
        }};
        response_edit.push_str(format!("\n[{}:{}]", discord_id.mention(), discord_id).as_str())
    } else if user.len() < 17 && user.chars().all(|c| c.is_ascii_digit()) {
        let details = match rbx_client.user_details(user.parse::<u64>().unwrap()).await {Ok(id) => id, Err(err) => {
            errors_vector.push(format!("Couldn't find turn discord id into roblox id for {}, details:\n{}", user, err));
            return (response_edit, errors_vector)
        }};
        response_edit.push_str(format!("\n[{}:{}]", details.username, details.id).as_str())
    } else if !user.chars().all(|c| c.is_ascii_digit()) {
        let user_search = match rbx_client.username_user_details(vec![user.to_string()], false).await {Ok(id) => id, Err(err) => {
            errors_vector.push(format!("Couldn't find user details for {}, details:\n{}", user, err));
            return (response_edit, errors_vector)
        }};
        for details in user_search {
            response_edit.push_str(format!("\n[{}:{}]", details.username, details.id).as_str())
        }
    }
    (response_edit, errors_vector)
}

#[poise::command(slash_command, prefix_command)]
/// Makes a Discord infraction log based on the Discord IDs inputted.
pub async fn false_infraction(
    ctx: Context<'_>,
    #[description = "Type of infraction."] #[rename = "type"] infraction_type: FalseInfTypes,
    #[description = "Moderator users (space-separated IDs)"] mod_users: String,
    #[description = "Affected users (space-separated IDs)"] affected_users: String,
    #[description = "Reason for invalidation."] reason: String,
) -> Result<(), Error> {
    ctx.reply("Making log(s), please standby!").await.unwrap();

    let mod_ids: Vec<UserId> = mod_users
        .split_whitespace()
        .filter_map(|id| UserId::from_str(id).ok())
        .collect();

    let mut affected_ids: Vec<&str> = affected_users
        .split_whitespace()
        .collect();

    if mod_ids.is_empty() || affected_ids.is_empty() {
        ctx.say("Error: No valid user IDs provided for moderators or affected users.").await?;
        return Ok(());
    }

    let mut affected_iter = 0;

    for (index, mod_id) in mod_ids.iter().enumerate() {
        if index + 1 == mod_ids.len() {
            let mut response = format!("[{}]\n[{}:{}]", infraction_type.name(), mod_id.to_user(&ctx.http()).await.unwrap().mention(), mod_id);
            for affected_id in &affected_ids {
                let result = do_affected_id(&ctx.data().rbx_client, affected_id).await;
                for err in result.1 {
                    ctx.say(err).await.unwrap();
                }
                response.push_str(result.0.as_str());
            }
            response.push_str(format!("\n[{}]", reason).as_str());
            ctx.say(response).await?;
        } else {
            let mut response = format!("[{}]\n[{}:{}]", infraction_type.name(), mod_id.to_user(&ctx.http()).await.unwrap().name, mod_id);
            let affected_id = affected_ids[affected_iter];
            let result = do_affected_id(&ctx.data().rbx_client, affected_id).await;
            for err in result.1 {
                ctx.say(err).await.unwrap();
            }
            response.push_str(result.0.as_str());
            response.push_str(format!("\n[{}]", reason).as_str());
            ctx.say(response).await?;
            if affected_ids.get(affected_iter + 1).is_some() {affected_ids.remove(affected_iter); affected_iter += 1};
        }
    }
    Ok(())
}