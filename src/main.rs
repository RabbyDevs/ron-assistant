use ::serenity::all::{
    ChannelId, Color, CreateAttachment, CreateMessage, GuildId, MessageId, MessageReference,
    ReactionType, RoleId,
};
use once_cell::sync::Lazy;
use poise::serenity_prelude as serenity;
use regex::Regex;
use reqwest::Client;
use roboat::ClientBuilder;
use serenity::{ActivityData, OnlineStatus};
use serenity::{UserId, prelude::*};
use std::{
    env,
    io::Write,
    str::FromStr,
    sync::{Arc, Mutex},
    vec,
};

mod main_modules;
use main_modules::{
    deleted_attachments::{self, AttachmentStore, AttachmentStoreDB},
    guide_updater::GuideSystem,
    helper, log_interactions,
    media::{
        QualityPreset, apply_mask, image_to_png_converter, png_to_gif_converter, video_convert,
        video_format_changer, video_to_gif_converter,
    },
    policy_updater::PolicySystem,
    timer::TimerSystem,
};
mod commands;
use commands::{
    guide_module::guide,
    info_module::{discord_info, get_info},
    log_module::{discord_log, false_infraction, probation_log, roblox_log, role_log},
    media_module::{convert_gif, convert_video, media_effects},
    playground::{auror, gamenight_helper},
    policy_module::policy,
    time_module::timed_role,
    update,
};

static_toml::static_toml! {
    static CONFIG = include_toml!("config.toml");
}

#[derive(Clone)]
pub struct Data {
    pub rbx_client: Arc<roboat::Client>,
    pub reqwest_client: Arc<Client>,
    pub number_regex: Arc<Regex>,
    pub timer_system: Arc<TimerSystem>,
    pub attachment_db: Arc<Mutex<AttachmentStoreDB>>,
    pub queued_logs: Arc<Mutex<Vec<LoggingQueue>>>,
    pub policy_system: PolicySystem,
    pub guide_system: GuideSystem,
    pub bot_color: Color,
    pub bot_avatar: String,
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

async fn do_image_logging(
    ctx: &serenity::Context,
    framework_data: Data,
    deleting_message: serenity::all::MessageId,
    guild_id: Option<GuildId>,
    channel_id: ChannelId,
) {
    let db_entry = match framework_data
        .attachment_db
        .lock()
        .unwrap()
        .get(deleting_message.to_string().as_str())
    {
        Some(entry) => entry,
        None => {
            return;
        }
    };

    for attachment in db_entry.attachments {
        let reqwest_client = framework_data.reqwest_client.clone();
        let data = framework_data.clone();
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if guild_id.is_some() && guild_id.unwrap().to_string() == *CONFIG.main.guild_id {
                let log_channel_id = ChannelId::new(
                    CONFIG
                        .modules
                        .logging
                        .attachment_logging_channel_id
                        .parse::<u64>()
                        .unwrap(),
                );
                let output_filename = format!("./.tmp/{}", attachment.filename);
                let response = reqwest_client.get(&attachment.url).send().await.unwrap();
                let bytes = response.bytes().await.unwrap();
                let mut file =
                    std::fs::File::create(&output_filename).expect("Failed to create input file");
                file.write_all(&bytes).expect("Failed to write input file");
                drop(file);
                let attachment = CreateAttachment::file(
                    &tokio::fs::File::open(&output_filename).await.unwrap(),
                    &attachment.filename,
                )
                .await
                .unwrap();
                let embed = helper::new_embed_from_template(&data)
                    .await
                    .title("Attachment Log")
                    .field(
                        "User",
                        format!("<@{}> - {}", db_entry.user_id, db_entry.user_id),
                        false,
                    )
                    .field(
                        "Sent on",
                        format!("<t:{}>", db_entry.created_at.unix_timestamp()),
                        false,
                    )
                    .field(
                        "Surrounding messages",
                        db_entry.message_id.link(channel_id, guild_id),
                        false,
                    );
                log_channel_id
                    .send_message(
                        &ctx.http,
                        CreateMessage::new().add_embed(embed).add_file(attachment),
                    )
                    .await
                    .unwrap();
                std::fs::remove_file(output_filename).unwrap();
            };
        });
    }

    framework_data
        .attachment_db
        .lock()
        .unwrap()
        .delete(deleting_message.to_string().as_str())
        .unwrap();
}

#[derive(Debug, Clone)]
pub struct LoggingQueue {
    pub message_id: MessageId,
}

impl LoggingQueue {
    pub async fn do_image_logging(
        &self,
        ctx: &serenity::Context,
        framework_data: Data,
        deleting_message: serenity::all::MessageId,
        guild_id: Option<GuildId>,
        channel_id: ChannelId,
    ) {
        do_image_logging(ctx, framework_data, deleting_message, guild_id, channel_id).await;
    }
}

static DODGED_FILE_FORMATS: Lazy<Vec<String>> = Lazy::new(|| {
    vec![
        "video/mp4".to_string(),
        "video/webm".to_string(),
        "video/quicktime".to_string(),
    ]
});

struct ReactionInfo {
    channel_id: ChannelId,
    message_id: MessageId,
    user_id: Option<UserId>,
    guild_id: Option<GuildId>,
    emoji: Option<ReactionType>,
}

async fn reaction_logging(
    ctx: &serenity::prelude::Context,
    framework_data: Data,
    event_type: &str,
    reaction_info: ReactionInfo,
) {
    let log_channel_id = ChannelId::new(
        CONFIG
            .modules
            .logging
            .reaction_logging_channel_id
            .parse()
            .unwrap(),
    );
    let mut embed_builder = helper::new_embed_from_template(&framework_data).await;
    let (channel_id, message_id, user_id, guild_id, emoji) = (
        reaction_info.channel_id,
        reaction_info.message_id,
        reaction_info.user_id,
        reaction_info.guild_id,
        reaction_info.emoji,
    );

    let emoji_url = match emoji {
        Some(ReactionType::Custom { animated, id, .. }) => {
            let extension = if animated { "gif" } else { "png" };
            format!("https://cdn.discordapp.com/emojis/{}.{}", id, extension)
        }
        Some(ReactionType::Unicode(_)) => String::new(),
        _ => String::new(),
    };

    let (title, color): (&str, (u8, u8, u8)) = match event_type {
        "add" => ("Reaction Added", (3, 252, 98)),
        "remove" => ("Reaction Removed", (252, 7, 3)),
        "remove_all" => ("All Reactions Removed", (77, 1, 0)),
        "remove_emoji" => ("Emoji Removed", (145, 2, 0)),
        _ => ("Reaction Event", (98, 32, 7)),
    };

    embed_builder = embed_builder
        .title(title)
        .field("Channel", channel_id.mention().to_string(), true)
        .field(
            "Message",
            message_id.link(channel_id, guild_id).to_string(),
            false,
        )
        .color(color);

    if let Some(emoji) = emoji {
        embed_builder = embed_builder.field("Emoji", emoji.to_string(), false);
    }

    if let Some(user_id) = user_id {
        embed_builder = embed_builder.field("Original User", user_id.mention().to_string(), true);
    }

    if !emoji_url.is_empty() {
        embed_builder = embed_builder.thumbnail(emoji_url);
    }

    if let Err(why) = log_channel_id
        .send_message(&ctx.http, CreateMessage::new().add_embed(embed_builder))
        .await
    {
        eprintln!("Error sending log message: {:?}", why);
    }
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => {
            println!("{} is connected!", data_about_bot.user.name);
            let ctx = ctx.clone();
            data.timer_system
                .set_event_handler(move |user_id: String, role_id: String| {
                    let ctx = ctx.clone();
                    Box::pin(async move {
                        let user_id = UserId::from_str(user_id.as_str()).expect("Invalid user ID");
                        let role_id = RoleId::from_str(role_id.as_str()).expect("Invalid role ID");

                        let guilds = ctx.cache.guilds();

                        for guild_id in guilds {
                            if let Ok(guild) = guild_id.to_partial_guild(&ctx).await {
                                if let Ok(member) = guild.member(&ctx.http, user_id).await {
                                    match member.remove_role(&ctx.http, role_id).await {
                                        Ok(()) => (),
                                        Err(err) => println!(
                                            "Couldn't remove role from user in {}, {}",
                                            guild_id, err
                                        ),
                                    };
                                }
                            }
                        }
                    })
                })
                .await;
            data.timer_system.start_timer_thread();

            // Start periodic cleanup task for expired modlogs
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5 * 60)); // Run every 5 minutes
                loop {
                    interval.tick().await;
                    let cleaned = log_interactions::cleanup_expired_modlogs();
                    if cleaned > 0 {
                        println!("Cleaned up {} expired modlog(s)", cleaned);
                    }
                }
            });
        }

        serenity::FullEvent::Message { new_message } => {
            if new_message.channel_id.to_string() == *CONFIG.modules.logging.cdn_channel_id
                || new_message.channel_id.to_string()
                    == *CONFIG.modules.logging.attachment_logging_channel_id
            {
                return Ok(());
            }

            // Check if this is a reply to a modlog image upload request
            if let Some(MessageReference {
                message_id: Some(replied_to_id),
                ..
            }) = &new_message.message_reference
            {
                let replied_msg_id = replied_to_id.get();
                let modlog_id_option = log_interactions::MESSAGE_TO_MODLOG
                    .read()
                    .get(&replied_msg_id)
                    .cloned();

                if let Some(modlog_id) = modlog_id_option {
                    // This is a reply to a modlog image upload request
                    if !new_message.attachments.is_empty() {
                        // Process the image upload for this modlog
                        let attachments = new_message.attachments.clone();
                        if let Err(err) = log_interactions::post_modlog_with_images(
                            ctx,
                            &modlog_id,
                            &attachments,
                        )
                        .await
                        {
                            eprintln!("Error posting modlog with images: {:?}", err);
                            let _ = new_message
                                .reply(ctx, "Failed to post modlog with images. The modlog may have expired.")
                                .await;
                        } else {
                            // Delete the user's message after successfully posting
                            let _ = new_message.delete(ctx).await;
                        }
                        // Remove the mapping since we've processed it
                        log_interactions::MESSAGE_TO_MODLOG.write().remove(&replied_msg_id);
                        return Ok(());
                    }
                }
            }

            if new_message.attachments.is_empty() {
                return Ok(());
            }

            let message = CreateMessage::new();
            let mut files = vec![];
            for attachment in &new_message.attachments {
                let output_filename = format!("./.tmp/{}", attachment.filename);
                let response = data
                    .reqwest_client
                    .get(&attachment.url)
                    .send()
                    .await
                    .unwrap();
                let bytes = response.bytes().await.unwrap();
                let mut file =
                    std::fs::File::create(&output_filename).expect("Failed to create input file");
                file.write_all(&bytes).expect("Failed to write input file");
                drop(file);
                files.push(
                    CreateAttachment::file(
                        &tokio::fs::File::open(&output_filename).await.unwrap(),
                        &attachment.filename,
                    )
                    .await
                    .unwrap(),
                );
                std::fs::remove_file(&output_filename).unwrap();
            }
            let log_channel_id = ChannelId::new(
                CONFIG
                    .modules
                    .logging
                    .cdn_channel_id
                    .parse::<u64>()
                    .unwrap(),
            );
            let final_msg = log_channel_id
                .send_message(&ctx.http, message.add_files(files))
                .await
                .unwrap();
            let user_id = new_message.author.id;
            let attachments = final_msg.attachments;
            let created_at = new_message.id.created_at();
            let message_id = new_message.id;
            let store = AttachmentStore {
                message_id,
                attachments,
                created_at,
                user_id,
            };

            for attachment in &new_message.attachments {
                let Some(content_type) = &attachment.content_type else {
                    continue;
                };
                if !content_type.contains("video/") || DODGED_FILE_FORMATS.contains(content_type) {
                    continue;
                }

                let new_message = new_message.clone();
                let attachment = attachment.clone();
                let ctx = ctx.clone();
                let reqwest_client = data.reqwest_client.clone();
                tokio::spawn(async move {
                    video_convert(new_message, ctx, reqwest_client, attachment).await;
                });
            }

            data.attachment_db.lock().unwrap().save(&store).unwrap();

            let message_id = new_message.id;
            let mut i = 0;
            while i < data.queued_logs.lock().unwrap().len() {
                let log = data.queued_logs.lock().unwrap().get(i).unwrap().clone();
                if log.message_id == message_id {
                    log.do_image_logging(
                        ctx,
                        framework.user_data.clone(),
                        message_id,
                        new_message.guild_id,
                        new_message.channel_id,
                    )
                    .await;
                    data.queued_logs.lock().unwrap().remove(i);
                }
                i += 1
            }
        }

        serenity::FullEvent::MessageDelete {
            channel_id,
            deleted_message_id,
            guild_id,
        } => {
            if channel_id.to_string() == *CONFIG.modules.logging.cdn_channel_id {
                return Ok(());
            }
            match data
                .attachment_db
                .lock()
                .unwrap()
                .get(deleted_message_id.to_string().as_str())
            {
                Some(entry) => entry,
                None => {
                    data.queued_logs.lock().unwrap().push(LoggingQueue {
                        message_id: *deleted_message_id,
                    });
                    return Ok(());
                }
            };
            do_image_logging(
                ctx,
                framework.user_data.clone(),
                *deleted_message_id,
                *guild_id,
                *channel_id,
            )
            .await;
        }

        serenity::FullEvent::GuildMemberAddition { new_member } => {
            let user_id = new_member.user.id.to_string();
            let timers = data.timer_system.list_user_timers(&user_id).await;
            for timer in timers {
                if let Ok(role_id) = data
                    .timer_system
                    .toggle_timer(&user_id, &timer.timer_id)
                    .await
                {
                    new_member
                        .add_role(
                            &ctx.http,
                            RoleId::new(role_id.unwrap().parse::<u64>().unwrap()),
                        )
                        .await
                        .unwrap();
                };
            }
        }

        serenity::FullEvent::GuildMemberRemoval { user, .. } => {
            let user_id = user.id.to_string();
            let timers = data.timer_system.list_user_timers(&user_id).await;
            for timer in timers {
                data.timer_system
                    .toggle_timer(&user_id, &timer.timer_id)
                    .await?;
            }
        }

        serenity::FullEvent::GuildBanAddition {
            banned_user,
            guild_id: _,
        } => {
            let timer_system = framework.user_data.timer_system.clone();
            let user_id = banned_user.id.to_string();

            let timers = timer_system.list_user_timers(&user_id).await;
            if !timers.is_empty() {
                for timer in timers {
                    if timer.delete_on_ban {
                        timer_system.delete_timer(&user_id, &timer.timer_id).await?;
                    }
                }
            }
        }

        serenity::FullEvent::ReactionAdd { add_reaction } => {
            reaction_logging(
                ctx,
                framework.user_data.clone(),
                "add",
                ReactionInfo {
                    channel_id: add_reaction.channel_id,
                    message_id: add_reaction.message_id,
                    user_id: Some(add_reaction.user_id.unwrap()),
                    guild_id: add_reaction.guild_id,
                    emoji: Some(add_reaction.emoji.clone()),
                },
            )
            .await;
        }

        serenity::FullEvent::ReactionRemove { removed_reaction } => {
            reaction_logging(
                ctx,
                framework.user_data.clone(),
                "remove",
                ReactionInfo {
                    channel_id: removed_reaction.channel_id,
                    message_id: removed_reaction.message_id,
                    user_id: Some(removed_reaction.user_id.unwrap()),
                    guild_id: removed_reaction.guild_id,
                    emoji: Some(removed_reaction.emoji.clone()),
                },
            )
            .await;
        }

        serenity::FullEvent::ReactionRemoveAll {
            channel_id,
            removed_from_message_id,
        } => {
            reaction_logging(
                ctx,
                framework.user_data.clone(),
                "remove_all",
                ReactionInfo {
                    channel_id: *channel_id,
                    message_id: *removed_from_message_id,
                    user_id: None,
                    guild_id: None,
                    emoji: None,
                },
            )
            .await;
        }

        serenity::FullEvent::ReactionRemoveEmoji { removed_reactions } => {
            reaction_logging(
                ctx,
                framework.user_data.clone(),
                "remove_emoji",
                ReactionInfo {
                    channel_id: removed_reactions.channel_id,
                    message_id: removed_reactions.message_id,
                    user_id: if removed_reactions.user_id.is_none() {
                        None
                    } else {
                        Some(removed_reactions.user_id.unwrap())
                    },
                    guild_id: removed_reactions.guild_id,
                    emoji: Some(removed_reactions.emoji.clone()),
                },
            )
            .await;
        }

        serenity::FullEvent::InteractionCreate { interaction } => {
            // Handle button clicks and modal submissions for modlog creation
            match interaction {
                serenity::Interaction::Component(component_interaction) => {
                    // Handle "Create Modlog" button clicks
                    if component_interaction
                        .data
                        .custom_id
                        .starts_with("create_roblox_log:")
                        && let Err(err) =
                            log_interactions::handle_create_log_button(ctx, component_interaction)
                                .await
                    {
                        eprintln!("Error handling create log button: {:?}", err);
                    }
                    if component_interaction
                        .data
                        .custom_id
                        .starts_with("select_log_type:")
                        && let Err(err) =
                            log_interactions::handle_select_log_type(ctx, component_interaction)
                                .await
                    {
                        eprintln!("Error handling create log button: {:?}", err);
                    }
                    // Handle "Upload Images" button clicks
                    if component_interaction
                        .data
                        .custom_id
                        .starts_with("upload_images:")
                        && let Err(err) =
                            log_interactions::handle_upload_images_button(ctx, component_interaction)
                                .await
                    {
                        eprintln!("Error handling upload images button: {:?}", err);
                    }
                    // Handle "Skip Images" button clicks
                    if component_interaction
                        .data
                        .custom_id
                        .starts_with("skip_images:")
                        && let Err(err) =
                            log_interactions::handle_skip_images_button(ctx, component_interaction)
                                .await
                    {
                        eprintln!("Error handling skip images button: {:?}", err);
                    }
                }
                serenity::Interaction::Modal(modal_interaction) => {
                    // Handle modal submissions for modlog creation
                    if modal_interaction
                        .data
                        .custom_id
                        .starts_with("roblox_log_modal:")
                        && let Err(err) =
                            log_interactions::handle_log_modal_submit(ctx, modal_interaction).await
                    {
                        eprintln!("Error handling log modal submit: {:?}", err);
                    }
                }
                _ => {}
            }
        }

        _ => {}
    }
    Ok(())
}

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tokio::time::sleep;

async fn remove_old_files() {
    let tmp_dir = Path::new("./.tmp");
    let now = SystemTime::now();
    let threshold = Duration::new(60, 0);

    if let Ok(entries) = fs::read_dir(tmp_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            };
            match fs::metadata(&path) {
                Ok(metadata) => {
                    if let Ok(modified_time) = metadata.modified() {
                        if let Ok(age) = now.duration_since(modified_time) {
                            if age >= threshold {
                                println!("Deleting file: {:?}", path);
                                if let Err(err) = fs::remove_file(&path) {
                                    eprintln!("Failed to delete {:?}: {}", path, err);
                                }
                            }
                        }
                    }
                }
                Err(err) => eprintln!("Failed to get metadata for {:?}: {}", path, err),
            }
        }
    } else {
        eprintln!("Failed to read .tmp directory");
    }
}

async fn periodic_cleanup() {
    loop {
        remove_old_files().await;
        sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::main]
async fn main() {
    deleted_attachments::start_attachment_db();
    std::fs::create_dir_all("./.tmp").unwrap();
    tokio::spawn(periodic_cleanup());
    let discord_api_key = &CONFIG.main.discord_api_key;
    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_PRESENCES
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::DIRECT_MESSAGE_TYPING
        | GatewayIntents::DIRECT_MESSAGE_REACTIONS
        | GatewayIntents::GUILD_MESSAGE_REACTIONS;

    let commands = vec![
        discord_log::discordlog(),
        roblox_log::robloxlog(),
        probation_log::probationlog(),
        role_log::rolelog(),
        get_info::getinfo(),
        update::update(),
        discord_info::discordinfo(),
        timed_role::timed_role(),
        false_infraction::false_infraction(),
        convert_video::convert_video(),
        convert_gif::gif(),
        media_effects::media(),
        policy::policy(),
        auror::id_to_mention(),
        gamenight_helper::gamenight_helper(),
        guide::guide(),
    ];

    let empty_commands: Vec<poise::Command<Data, Error>> = vec![];

    let color_string = CONFIG.main.color;
    let colors: Vec<u8> = color_string
        .split(',')
        .map(|s| u8::from_str(s.trim()).expect("Failed to parse color component"))
        .collect();

    let (r, g, b) = (colors[0], colors[1], colors[2]);

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands,
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            let activity =
                ActivityData::custom(format!("Running on v{}!", env!("CARGO_PKG_VERSION")));
            let status = OnlineStatus::Online;

            ctx.set_presence(Some(activity), status);
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &empty_commands).await?;
                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    GuildId::new(u64::from_str(CONFIG.main.guild_id).unwrap()),
                )
                .await?;
                Ok(Data {
                    rbx_client: Arc::new(ClientBuilder::new().build()),
                    reqwest_client: Arc::new(Client::new()),
                    number_regex: Arc::new(Regex::new(r"[^\d\s]").expect("Failed to create regex")),
                    timer_system: Arc::new(TimerSystem::new("./dbs/timer_system").await.unwrap()),
                    attachment_db: AttachmentStoreDB::get_instance(),
                    queued_logs: Arc::new(Mutex::new(vec![])),
                    policy_system: PolicySystem::init("./dbs/policy_system").unwrap(),
                    guide_system: GuideSystem::init("./dbs/guide_system").unwrap(),
                    bot_color: Color::from_rgb(r, g, b),
                    bot_avatar: ready
                        .user
                        .avatar_url()
                        .unwrap_or_else(|| ready.user.default_avatar_url()),
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(discord_api_key, intents)
        .framework(framework)
        .await
        .expect("client start err");

    client.start().await.unwrap();
}
