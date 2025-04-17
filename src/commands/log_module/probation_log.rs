use poise::ChoiceParameter;
use serenity::User;

use super::{Context, Error, helper, UserId, Mentionable, serenity, FromStr};

#[derive(Debug, poise::ChoiceParameter)]
pub enum ProbationTypes {
    #[name = "Roblox Ban"]
    RobloxBan,
    #[name = "Discord Ban"]
    DiscordBan
}

#[poise::command(slash_command, prefix_command)]
/// Makes a probation log based on the Discord IDs inputted.
pub async fn probationlog(
    ctx: Context<'_>,
    #[description = "Users for the command, only accepts Discord ids."] users: String,
    #[description = "Type of infraction."] #[rename = "type"] infraction_type: ProbationTypes,
    #[description = "Reason for infraction."] reason: String,
    #[description = "Duration of the probation (e.g., '1h', '2d', '1w')."] duration: String
) -> Result<(), Error> {
    ctx.reply("Making logs, please standby!").await?;
    let purified_users = ctx.data().number_regex.replace_all(users.as_str(), "");
    if purified_users.is_empty() {
        ctx.say("Command failed; no users inputted, or users improperly inputted.").await?;
        return Ok(());
    }
    let users = purified_users.split(' ');
    let reasons = reason.split('|').map(str::to_string).collect::<Vec<String>>();
    let type_string = format!("[{}]\n\n", infraction_type.name());

    let mut duration_errors = Vec::new();
    let raw_durations = duration.split('|').map(str::to_string).collect::<Vec<String>>();
    let mut durations = Vec::new();
    let duration_handler = tokio::spawn(async move {
        for duration in raw_durations {
            let (current_time, unix_timestamp, timestamp_string) = match helper::duration_conversion(duration).await {
                Ok((current_time, unix_timestamp, timestamp_string)) => (current_time, unix_timestamp, timestamp_string),
                Err(err) => {
                    duration_errors.push(err);
                    continue
                },
            };
            durations.push(format!("[{} (<t:{}:D> - <t:{}:D>)]", timestamp_string, current_time, unix_timestamp))
        }
        (durations, duration_errors)
    });

    let mut reason_number = 0;
    let mut response_vec = Vec::new();
    for snowflake in users {
        let userid: UserId = UserId::from_str(snowflake).expect("something went wrong.");
        let reqwest_client = ctx.data().reqwest_client.clone();
        let rbx_client = ctx.data().rbx_client.clone();
        let roblox_handler = tokio::spawn(async move {
            let mut roblox_errors = Vec::new();
            let roblox_id = match helper::discord_id_to_roblox_id(&reqwest_client, userid).await {
                Ok(roblox_id) => roblox_id,
                Err(_) => {roblox_errors.push(format!("A error occured on Bloxlink's end when getting {}'s Roblox id. The user may be not verified with Bloxlink or Bloxlink is down.", userid));
                "null".to_string()}
            };
            let roblox_user = if roblox_id != *"null".to_string() {rbx_client.user_details(roblox_id.parse::<u64>().expect("err")).await.expect("err").username} else { "null".to_string() };
            (roblox_id, roblox_user, roblox_errors)
        });
        let user: User = match userid.to_user(ctx).await {
            Ok(user) => user,
            Err(_) => {
                ctx.say(format!("A error occured attempting to process user `{}` skipping user's log.", snowflake)).await?;
                continue
            }
        };
        let (roblox_user, roblox_id, roblox_errors) = roblox_handler.await.unwrap();
        for error in roblox_errors {ctx.say(error).await?;}
        response_vec.push(format!("{}[{}:{} - {}:{}]\n\n[{}]\n\n", type_string, user.mention(), user.id, roblox_user, roblox_id, reasons[reason_number]));
        if reasons.get(reason_number + 1).is_some() { reason_number += 1 }
    }

    let (durations, duration_errors) = duration_handler.await.unwrap();
    for error in duration_errors {
        ctx.say(error).await?;
    }
    let mut duration_number = 0;
    for response in response_vec {
        let response = format!("{}{}", response, match durations.get(duration_number) { Some(dur) => dur, None => continue });
        ctx.say(response.clone()).await?;
        let extra_response = format!("{}{}", "# Probation\n", response);
        ctx.say(extra_response).await?;
        if durations.get(duration_number + 1 ).is_some() { duration_number += 1 }
    }
    Ok(())
}