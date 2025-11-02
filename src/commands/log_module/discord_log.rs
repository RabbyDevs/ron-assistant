// Command for making discord-side logs
use poise::ChoiceParameter;
use serenity::User;

use super::{Context, Error, helper, UserId, Mentionable, serenity, FromStr};

#[derive(Debug, poise::ChoiceParameter)]
pub enum DiscordInfTypes {
    Ban,
    #[name = "Temporary Ban"]
    TempBan,
    Kick,
    Mute,
    Warn
}

#[poise::command(slash_command, prefix_command)]
/// Makes a Discord infraction log based on the Discord IDs inputted.
pub async fn discordlog(
    ctx: Context<'_>,
    #[description = "User ids for the command."] users: String,
    #[description = "Type of infraction."] #[rename = "type"] infraction_type: DiscordInfTypes,
    #[description = "Reason for infraction."] reason: String,
    #[description = "Note for the infraction."] note: Option<String>,
    #[description = "Multimessage mode allows creation of multiple logs from 1 command."] multimessage: Option<bool>
) -> Result<(), Error> {
    ctx.reply("Making logs, please standby!").await?;
    let multimessage = multimessage.unwrap_or_default();
    let purified_users = ctx.data().number_regex.replace_all(users.as_str(), "");
    if purified_users.is_empty() {
        ctx.say("Command failed; no users inputted, or users improperly inputted.").await?;
        return Ok(());
    }
    let users = purified_users.split(' ');
    let reasons = reason.split('|').map(str::to_string).collect::<Vec<String>>();
    let notes = note.unwrap_or_default().split('|').map(str::to_string).collect::<Vec<String>>();
    let mut users_string = String::new();
    let mut user_string_vec: Vec<String> = Vec::new();
    for snowflake in users {
        let userid: UserId = UserId::from_str(snowflake).expect("something went wrong.");
        let user: User = match userid.to_user(ctx).await {
            Ok(user) => user,
            Err(_) => {
                ctx.say(format!("A error occured attempting to process user `{}` skipping user's log.", snowflake)).await?;
                continue
            }
        };
        let mut user_string = String::new();
        user_string.push_str(format!("[{}:{}", user.mention(), user.id).as_str());
        let roblox_id = if infraction_type.name() == "Ban" { match helper::discord_id_to_roblox_id(&ctx.data().reqwest_client, user.id).await {Ok(id) => id, Err(err) => {
            ctx.say(err).await?;
            "null".to_string()
        }}} else { "null".to_string() };
        let roblox_user = if roblox_id != *"null".to_string() {ctx.data().rbx_client.user_details(roblox_id.parse::<u64>().expect("err")).await?.username} else { "null".to_string() };
        if infraction_type.name() == "Ban" { user_string.push_str(format!(" - {}:{}]\n", roblox_user, roblox_id).as_str()) } else { user_string.push_str("]\n") }
        if !multimessage {users_string.push_str(user_string.as_str())} else {user_string_vec.push(user_string)}
    }
    let type_string = format!("[{}]\n", infraction_type.name());
    if !multimessage {
        let reason_string = format!("[{}]", reasons[0]);
        let note_string = if !notes[0].is_empty() {format!("\nNote: {}", notes[0])} else {String::new()};
        let response = format!("{}{}{}{}", type_string, users_string, reason_string, note_string);
        ctx.say(response).await?;
    } else {
        let mut reason_number = 0;
        let mut note_number = 0;
        for user_string in user_string_vec {
            let reason_string = format!("[{}]", reasons[reason_number]);
            let note_string = if !notes[note_number].is_empty() {format!("\nNote: {}", notes[note_number])} else {String::new()};
            let response = format!("{}{}{}{}", type_string, user_string, reason_string, note_string);
            ctx.say(response).await?;
            if reasons.get(reason_number + 1).is_some() { reason_number += 1 }
            if notes.get(note_number + 1 ).is_some() { note_number += 1 }
        }
    }
    Ok(())
}