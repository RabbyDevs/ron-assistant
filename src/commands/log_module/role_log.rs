// Command for making discord-side logs
use poise::ChoiceParameter;
use serenity::User;

use super::{Context, Error, helper, UserId, Mentionable, serenity, FromStr};

#[derive(Debug, poise::ChoiceParameter)]
pub enum RoleEnums {
    #[name = "Dedicated Player"]
    DedicatedPlayer,
    Collector,
    Grandmaster,
    Gamebanned,
    #[name = "VIP Blacklist"]
    VIPBlacklist,
    #[name = "Debate Blacklist"]
    DebateBlacklist,
    #[name = "Event Blacklist"]
    EventBlacklist,
    #[name = "Suggestion Blacklist"]
    SuggestionBlacklist,
    #[name = "VC Blackist"]
    VCBlacklist,
    #[name = "Application Blacklist"]
    ApplicationBlacklist,
    #[name = "Creations Blacklist"]
    CreationsBlacklist,
    #[name = "Wiki Blacklist"]
    WikiBlacklist,
    #[name = "Retired Staff"]
    RetiredStaff,
    #[name = "Seasoned Staff"]
    SeasonedStaff,
    #[name = "Veteran Staff"]
    VeteranStaff,
    #[name = "Legacy Staff"]
    LegacyStaff,
    #[name = "Challenges Blacklist"]
    ChallengesBlacklist,
    #[name = "Strategies Blacklist"]
    StrategiesBlacklist,
    #[name = "Ticket Blacklist"]
    TicketBlacklist,
    #[name = "Feedback Blacklist"]
    FeedbackBlacklist,
    #[name = "Gamebot Blacklist"]
    GamebotBlacklist,
    #[name = "Trusted VIP Host"]
    TrustedVIPHost,
    #[name = "Ask for Help Blacklist"]
    AskForHelpBlacklist,
    #[name = "Content Creator"]
    ContentCreator,
}
#[derive(Debug, poise::ChoiceParameter)]
pub enum LogType {
    Addition,
    Removal,
}

#[poise::command(slash_command, prefix_command)]
/// Makes a role addition or deletion log based on the Discord IDs inputted.
pub async fn rolelog(
    ctx: Context<'_>,
    #[description = "Users for the command, accepts only Discord ids."] users: String,
    #[description = "The role."] #[rename = "type"] infraction_type: LogType,
    #[description = "The role."] #[rename = "role"] role: RoleEnums,
    #[description = "Optional reason for giving the role."] reason: Option<String>,
) -> Result<(), Error> {

    
    ctx.reply("Making logs, please standby!").await?;
    let purified_users = ctx.data().number_regex.replace_all(users.as_str(), "");
    if purified_users.is_empty() {
        ctx.say("Command failed; no users inputted, or users improperly inputted.").await?;
        return Ok(());
    }
    let users = purified_users.split(' ');
    let reason = reason.unwrap_or_default();
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
        let mut response = format!("[{}]\n[{}]\n[{}:{} - {}:{}]", infraction_type.name(), role.name(), user.mention(), user.id, roblox_user, roblox_id);
        if !reason.is_empty() {response.push_str(format!("\n[{}]", reason).as_str())}
        ctx.say(response).await?;
    }
    Ok(())
}