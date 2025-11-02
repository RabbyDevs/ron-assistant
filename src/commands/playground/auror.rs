// Command for making discord-side logs
use poise::CreateReply;
use serenity::User;

use super::{Context, Error, UserId, serenity, FromStr};

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
/// Makes a ephermal message with all the inputted user ids in mention form.
pub async fn id_to_mention(
    ctx: Context<'_>,
    #[description = "User ids for the command."] users: String,
    #[description = "No multi-line?"] no_multiline: Option<bool>
) -> Result<(), Error> {
    ctx.reply("Formulating ids to mentions, standby!").await?;
    let no_multiline = no_multiline.unwrap_or_default();

    let purified_users = ctx.data().number_regex.replace_all(users.as_str(), "");
    if purified_users.is_empty() {
        ctx.say("Command failed; no users inputted, or users improperly inputted.").await?;
        return Ok(());
    }
    let users = purified_users.split(' ');
    let mut users_string = String::new();
    for snowflake in users {
        let userid: UserId = UserId::from_str(snowflake).expect("something went wrong.");
        let user: User = match userid.to_user(ctx).await {
            Ok(user) => user,
            Err(_) => {
                ctx.say(format!("A error occured attempting to process user `{}` skipping user's log.", snowflake)).await?;
                continue
            }
        };
        if !no_multiline { users_string.push_str(format!("<\\@{}>\n", user.id).as_str()); } else { users_string.push_str(format!("<\\@{}>", user.id).as_str()); }
    }

    let reply = CreateReply::default().content(users_string).ephemeral(true);
    ctx.send(reply).await?;
    Ok(())
}