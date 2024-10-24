use std::{env, io::Write, str::FromStr, sync::{Arc, Mutex}, vec};
use once_cell::sync::Lazy;
use regex::Regex;
use roboat::ClientBuilder;
use ::serenity::all::{ChannelId, Color, CreateAttachment, CreateEmbed, CreateEmbedFooter, CreateMessage, GuildId, MessageId, ReactionType, RoleId};
use serenity::{ActivityData, OnlineStatus};
use serenity::{prelude::*, UserId};
use poise::serenity_prelude as serenity;
use reqwest::Client;

mod main_modules;
use main_modules::{helper, media::{video_convert, video_format_changer, video_to_gif_converter, image_to_png_converter, png_to_gif_converter, QualityPreset, apply_mask}, timer::TimerSystem, deleted_attachments::{self, AttachmentStoreDB, AttachmentStore}, policy_updater::PolicySystem};
mod commands;
use commands::{
    info_module::{
        discord_info,
        get_info
    }, log_module::{
        discord_log, 
        false_infraction, 
        probation_log, 
        roblox_log, 
        role_log
    }, media_module::{
        convert_gif, convert_video, media_effects
    }, playground::auror, policy_module::policy, time_module::timed_role, update
};

static_toml::static_toml! {
    static CONFIG = include_toml!("config.toml");
}


struct Data {
    pub rbx_client: Arc<roboat::Client>,
    pub reqwest_client: Arc<Client>,
    pub number_regex: Arc<Regex>,
    pub timer_system: Arc<TimerSystem>,
    pub attachment_db: Arc<Mutex<AttachmentStoreDB>>,
    pub queued_logs: Arc<Mutex<Vec<LoggingQueue>>>,
    pub policy_system: PolicySystem,
    pub bot_color: Color
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

async fn do_image_logging(ctx: &serenity::Context, bot_icon: String, bot_color: Color, reqwest_client: Arc<reqwest::Client>, attachment_db: Arc<Mutex<AttachmentStoreDB>>, deleting_message: serenity::all::MessageId, guild_id: Option<GuildId>, channel_id: ChannelId) {
    let db_entry = match attachment_db.lock().unwrap().get(deleting_message.to_string().as_str()) {
        Some(entry) => entry,
        None => {
            return;
        }
    };

    for attachment in db_entry.attachments {
        let ctx = ctx.clone();
        let guild_id = guild_id.clone();
        let reqwest_client = reqwest_client.clone();
        let bot_icon= bot_icon.clone();
        tokio::spawn(async move {
            if guild_id.is_some() && guild_id.unwrap().to_string() == CONFIG.modules.logging.guild_id.to_string() {
                let log_channel_id = ChannelId::new(CONFIG.modules.logging.attachment_logging_channel_id.parse::<u64>().unwrap());
                let output_filename = format!("./.tmp/{}", attachment.filename);
                let response = reqwest_client.get(&attachment.url).send().await.unwrap();
                let bytes = response.bytes().await.unwrap();
                let mut file = std::fs::File::create(&output_filename).expect("Failed to create input file");
                file.write_all(&bytes).expect("Failed to write input file");
                drop(file);
                let attachment = CreateAttachment::file(&tokio::fs::File::open(&output_filename).await.unwrap(), &attachment.filename).await.unwrap();
                let footer = CreateEmbedFooter::new("Made by RabbyDevs, with 🦀 and ❤️.")
                    .icon_url(bot_icon);
                let embed = CreateEmbed::new().title("Attachment Log")
                    .field("User", format!("<@{}> - {}", db_entry.user_id, db_entry.user_id), false)
                    .field("Sent on", format!("<t:{}>", db_entry.created_at.unix_timestamp()), false)
                    .field("Surrounding messages", db_entry.message_id.link(channel_id, guild_id), false)
                    .color(bot_color)
                    .footer(footer);
                log_channel_id.send_message(&ctx.http, CreateMessage::new().add_embed(embed).add_file(attachment)).await.unwrap();
                std::fs::remove_file(output_filename).unwrap();
            };
        });
    }
    
    attachment_db.lock().unwrap().delete(deleting_message.to_string().as_str()).unwrap();
}

#[derive(Debug, Clone)]
pub struct LoggingQueue {
    pub message_id: MessageId
}

impl LoggingQueue {
    pub async fn do_image_logging(
        &self,
        ctx: &serenity::Context,
        bot_icon: String,
        bot_color: Color,
        reqwest_client: Arc<reqwest::Client>,
        attachment_db: Arc<Mutex<AttachmentStoreDB>>,
        deleting_message: serenity::all::MessageId,
        guild_id: Option<GuildId>,
        channel_id: ChannelId
    ) {
        do_image_logging(ctx, bot_icon, bot_color, reqwest_client, attachment_db, deleting_message, guild_id, channel_id).await;
    }
}

static DODGED_FILE_FORMATS: Lazy<Vec<String>> = Lazy::new(|| vec!["video/mp4".to_string(), "video/webm".to_string(), "video/quicktime".to_string()]);

async fn reaction_logging(
    ctx: &serenity::prelude::Context,
    bot_image: String,
    event_type: &str, 
    user_id: Option<UserId>, 
    channel_id: ChannelId, 
    message_id: MessageId, 
    guild_id: Option<GuildId>, 
    emoji: Option<&ReactionType>
) {
    let log_channel_id = ChannelId::new(CONFIG.modules.logging.reaction_logging_channel.parse().unwrap());
    let mut embed_builder = CreateEmbed::new();
    
    let emoji_url = match emoji {
        Some(ReactionType::Custom { animated, id, .. }) => {
            let extension = if *animated { "gif" } else { "png" };
            format!("https://cdn.discordapp.com/emojis/{}.{}", id, extension)
        },
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

    let footer = CreateEmbedFooter::new("Made by RabbyDevs, with 🦀 and ❤️.")
    .icon_url(bot_image);

    embed_builder = embed_builder
        .color(Color::from_rgb(color.0, color.1, color.2))
        .title(title)
        .field("Channel", channel_id.mention().to_string(), true)
        .field("Message", format!("{}", message_id.link(channel_id, guild_id)), false)
        .footer(footer);

    if let Some(emoji) = emoji {
        embed_builder = embed_builder.field("Emoji", emoji.to_string(), false);
    }

    if let Some(user_id) = user_id {
        embed_builder = embed_builder.field("Original User", user_id.mention().to_string(), true);
    }

    if !emoji_url.is_empty() {
        embed_builder = embed_builder.thumbnail(emoji_url);
    }

    if let Err(why) = log_channel_id.send_message(&ctx.http, CreateMessage::new().add_embed(embed_builder)).await {
        eprintln!("Error sending log message: {:?}", why);
    }
}


async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot,  .. } => {
            println!("{} is connected!", data_about_bot.user.name);
            let ctx = ctx.clone();
            data.timer_system.set_event_handler(move |user_id: String, role_id: String| {
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
                                    Err(err) => println!("Couldnt remove role from user in {}, {}", guild_id, err)
                                };
                            }
                        }
                    }
                })
            }).await;
            data.timer_system.start_timer_thread();
        }

        serenity::FullEvent::Message { new_message } => {
            if new_message.channel_id.to_string() == CONFIG.modules.logging.cdn_channel_id.to_string() || new_message.channel_id.to_string() == CONFIG.modules.logging.attachment_logging_channel_id.to_string() {
                return Ok(());
            }
            if new_message.attachments.is_empty() {
                return Ok(());
            }
    
            let message = CreateMessage::new();
            let mut files = vec![];
            for attachment in &new_message.attachments {
                let output_filename = format!("./.tmp/{}", attachment.filename);
                let response = data.reqwest_client.get(&attachment.url).send().await.unwrap();
                let bytes = response.bytes().await.unwrap();
                let mut file = std::fs::File::create(&output_filename).expect("Failed to create input file");
                file.write_all(&bytes).expect("Failed to write input file");
                drop(file);
                files.push(CreateAttachment::file(&tokio::fs::File::open(&output_filename).await.unwrap(), &attachment.filename).await.unwrap());
                std::fs::remove_file(&output_filename).unwrap();
            }
            let log_channel_id = ChannelId::new(CONFIG.modules.logging.cdn_channel_id.parse::<u64>().unwrap());
            let final_msg = log_channel_id.send_message(&ctx.http, message.add_files(files)).await.unwrap();
            let user_id = new_message.author.id;
            let attachments = final_msg.attachments;
            let created_at = new_message.id.created_at();
            let message_id = new_message.id;
            let store = AttachmentStore {
                message_id,
                attachments,
                created_at,
                user_id
            };
    
            for attachment in &new_message.attachments {
                let Some(content_type) = &attachment.content_type else { continue };
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
                    log.do_image_logging(&ctx,  _framework.bot_id.to_user(ctx.http()).await?.avatar_url().unwrap(), data.bot_color, data.reqwest_client.clone(), data.attachment_db.clone(), message_id, new_message.guild_id, new_message.channel_id).await;
                    data.queued_logs.lock().unwrap().remove(i);
                }
                i += 1
            }
        }

        serenity::FullEvent::MessageDelete { channel_id, deleted_message_id, guild_id } => {
            if channel_id.to_string() == CONFIG.modules.logging.cdn_channel_id.to_string() {
                return Ok(());
            }
            match data.attachment_db.lock().unwrap().get(deleted_message_id.to_string().as_str()) {
                Some(entry) => entry,
                None => {
                    data.queued_logs.lock().unwrap().push(LoggingQueue {
                        message_id: *deleted_message_id
                    });
                    return Ok(());
                }
            };
            do_image_logging(ctx, _framework.bot_id.to_user(ctx.http()).await?.avatar_url().unwrap(), data.bot_color, data.reqwest_client.clone(), data.attachment_db.clone(), *deleted_message_id, *guild_id, *channel_id).await;
        }

        serenity::FullEvent::GuildMemberAddition { new_member } => {
            match data.timer_system.resume_timer(new_member.user.id.to_string().as_str()).await {
                Ok(role_id) => {
                    new_member.add_role(&ctx.http, RoleId::new(role_id.parse::<u64>().unwrap())).await.unwrap();
                    ()
                },
                Err(_) => {
                    ()
                }
            };
        }

        serenity::FullEvent::GuildMemberRemoval { user, .. } => {
            match data.timer_system.pause_timer(user.id.to_string().as_str()).await {
                Ok(()) => {()},
                Err(_) => {()}
            };
        }
        
        serenity::FullEvent::ReactionAdd { add_reaction } => {
            reaction_logging(
                ctx, 
                _framework.bot_id.to_user(ctx.http()).await?.avatar_url().unwrap(),
                "add", 
                Some(add_reaction.user_id.unwrap()), 
                add_reaction.channel_id, 
                add_reaction.message_id, 
                add_reaction.guild_id, 
                Some(&add_reaction.emoji)
            ).await;
        }

        serenity::FullEvent::ReactionRemove { removed_reaction } => {
            reaction_logging(
                ctx, 
                _framework.bot_id.to_user(ctx.http()).await?.avatar_url().unwrap(),
                "remove", 
                Some(removed_reaction.user_id.unwrap()), 
                removed_reaction.channel_id, 
                removed_reaction.message_id, 
                removed_reaction.guild_id, 
                Some(&removed_reaction.emoji)
            ).await;
        }

        serenity::FullEvent::ReactionRemoveAll { channel_id, removed_from_message_id } => {
            reaction_logging(
                ctx, 
                _framework.bot_id.to_user(ctx.http()).await?.avatar_url().unwrap(),
                "remove_all", 
                None, 
                *channel_id, 
                *removed_from_message_id, 
                None, 
                None
            ).await;
        }

        serenity::FullEvent::ReactionRemoveEmoji { removed_reactions } => {
            reaction_logging(
                ctx, 
                _framework.bot_id.to_user(ctx.http()).await?.avatar_url().unwrap(),
                "remove_emoji", 
                None, 
                removed_reactions.channel_id, 
                removed_reactions.message_id, 
                removed_reactions.guild_id, 
                Some(&removed_reactions.emoji)
            ).await;
        }

        _ => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    deleted_attachments::start_attachment_db();
    std::fs::create_dir_all("./.tmp").unwrap();
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
                auror::id_to_mention()
            ];

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
                Box::pin(event_handler(ctx, event, framework, &data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            let activity = ActivityData::custom(format!("Running on v{}!", env!("CARGO_PKG_VERSION")));
            let status = OnlineStatus::Online;

            ctx.set_presence(Some(activity), status);
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    rbx_client: Arc::new(ClientBuilder::new().build()),
                    reqwest_client: Arc::new(Client::new()),
                    number_regex: Arc::new(Regex::new(r"[^\d\s]").expect("Failed to create regex")),
                    timer_system: Arc::new(TimerSystem::new("probation_role").unwrap()),
                    attachment_db: AttachmentStoreDB::get_instance(),
                    queued_logs: Arc::new(Mutex::new(vec![])),
                    policy_system: PolicySystem::init("./policy_system").unwrap(),
                    bot_color: Color::from_rgb(r, g, b)
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
