use poise::SlashArgument;
use serenity::all::{
    ComponentInteraction, ComponentInteractionDataKind, Context, CreateActionRow, CreateButton,
    CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage, CreateModal,
    CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption, InputTextStyle, ModalInteraction,
    ButtonStyle, Attachment,
};
use std::error::Error as StdError;
use std::sync::Arc;
use parking_lot::RwLock;
use once_cell::sync::Lazy;
use std::collections::HashMap;

use crate::commands::log_module::roblox_log::RobloxInfTypes;

/// Temporary storage for pending modlog data awaiting image uploads
/// Structure: {unique_id} -> {username, roblox_id, action_type, reason, note, message_id}
pub static PENDING_MODLOGS: Lazy<Arc<RwLock<HashMap<String, PendingModlog>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Mapping of bot message IDs to modlog IDs for image upload tracking
/// Structure: {bot_message_id} -> {modlog_id}
pub static MESSAGE_TO_MODLOG: Lazy<Arc<RwLock<HashMap<u64, String>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

#[derive(Clone, Debug)]
pub struct PendingModlog {
    pub username: String,
    pub roblox_id: String,
    pub action_type: String,
    pub reason: String,
    pub note: String,
    pub message_id: u64,
    pub channel_id: u64,
}

fn build_enum_options() -> Vec<CreateSelectMenuOption> {
    RobloxInfTypes::choices()
        .into_iter()
        .map(|choice| CreateSelectMenuOption::new(choice.name.clone(), choice.name.clone()))
        .collect()
}

/// Handles the "Create Modlog" button click - opens a modal for input
pub async fn handle_create_log_button(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();
    if parts.len() < 3 {
        return Err("Invalid button custom_id format".into());
    }

    let username = parts[1];
    let roblox_id = parts[2];

    let select_menu = CreateSelectMenu::new(
        format!("select_log_type:{}:{}", username, roblox_id),
        CreateSelectMenuKind::String {
            options: build_enum_options(),
        },
    )
    .placeholder("Select log type...");

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Please select a log type:")
                    .components(vec![CreateActionRow::SelectMenu(select_menu)])
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

pub async fn handle_select_log_type(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();

    if parts.len() < 3 {
        return Err(format!(
            "Invalid select custom_id format, custom_id {}",
            interaction.data.custom_id
        )
        .into());
    }

    let username = parts[1];
    let roblox_id = parts[2];
    let selected_log_type = match &interaction.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => &values[0],
        _ => panic!("unexpected interaction data kind"),
    };

    let modal = CreateModal::new(
        format!(
            "roblox_log_modal:{}:{}:{}",
            username, roblox_id, selected_log_type
        ),
        "Create Roblox Moderation Log",
    )
    .components(vec![
        CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Paragraph, "Reason", "reason")
                .placeholder("Enter the reason for this action...")
                .required(true),
        ),
        CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Paragraph, "Note (Optional)", "note")
                .placeholder("Add any additional notes...")
                .required(false),
        ),
    ]);
    interaction
        .create_response(ctx, CreateInteractionResponse::Modal(modal))
        .await?;

    Ok(())
}

/// Handles the modal submission - creates buttons for image upload or skip
pub async fn handle_log_modal_submit(
    ctx: &Context,
    interaction: &ModalInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Parse custom_id: "roblox_log_modal:{username}:{roblox_id}:{action_type}"
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();

    if parts.len() < 4 {
        return Err("Invalid modal custom_id format".into());
    }

    let username = parts[1];
    let roblox_id = parts[2];
    let action_type = parts[3];

    // Extract form inputs
    let mut reason = String::new();
    let mut note = String::new();

    for row in &interaction.data.components {
        for component in &row.components {
            if let serenity::all::ActionRowComponent::InputText(input) = component {
                match input.custom_id.as_str() {
                    "reason" => reason = input.value.clone().unwrap_or_default(),
                    "note" => note = input.value.clone().unwrap_or_default(),
                    _ => {}
                }
            }
        }
    }

    // Generate unique ID for this modlog
    let unique_id = uuid::Uuid::new_v4().to_string();

    // Store the pending modlog data
    let pending_modlog = PendingModlog {
        username: username.to_string(),
        roblox_id: roblox_id.to_string(),
        action_type: action_type.to_string(),
        reason: reason.clone(),
        note: note.clone(),
        message_id: interaction.message.as_ref().map(|m| m.id.get()).unwrap_or(0),
        channel_id: interaction.channel_id.get(),
    };

    PENDING_MODLOGS
        .write()
        .insert(unique_id.clone(), pending_modlog);

    // Create action buttons
    let upload_button = CreateButton::new(format!("upload_images:{}", unique_id))
        .label("Upload Images")
        .style(ButtonStyle::Primary);

    let skip_button = CreateButton::new(format!("skip_images:{}", unique_id))
        .label("Skip Images")
        .style(ButtonStyle::Secondary);

    // Build preview of the log
    let user_info = format!("[{}:{}]", username, roblox_id);
    let note_string = if !note.is_empty() {
        format!("\nNote: {}", note)
    } else {
        String::new()
    };

    let preview_message = format!(
        "**Modlog Preview:**\n```\n[{}]\n{}\n[{}]{}```\n\nWould you like to add images?",
        action_type, user_info, reason, note_string
    );

    // Respond with buttons
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(preview_message)
                    .components(vec![CreateActionRow::Buttons(vec![
                        upload_button,
                        skip_button,
                    ])])
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

/// Handles the "Upload Images" button click
pub async fn handle_upload_images_button(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();
    if parts.len() < 2 {
        return Err("Invalid button custom_id format".into());
    }

    let modlog_id = parts[1];

    // Verify the modlog exists
    if !PENDING_MODLOGS.read().contains_key(modlog_id) {
        return Err("Modlog not found. It may have expired.".into());
    }

    // Acknowledge the interaction with ephemeral message
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Waiting for your images...")
                    .ephemeral(true),
            ),
        )
        .await?;

    // Send a regular channel message that can be replied to
    let channel_id = interaction.channel_id;
    let user_mention = format!("<@{}>", interaction.user.id);
    let message = channel_id
        .send_message(
            ctx,
            serenity::all::CreateMessage::new().content(format!(
                "{} **Please reply to this message with images or attachments.**\n\
                 I'll wait for your attachments and then post the modlog with them.\n\n\
                 You have **5 minutes** to send your images.",
                user_mention
            )),
        )
        .await?;

    // Store the mapping of message ID to modlog ID
    MESSAGE_TO_MODLOG
        .write()
        .insert(message.id.get(), modlog_id.to_string());

    Ok(())
}

/// Handles the "Skip Images" button click
pub async fn handle_skip_images_button(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();
    if parts.len() < 2 {
        return Err("Invalid button custom_id format".into());
    }

    let modlog_id = parts[1];

    // Retrieve and remove the pending modlog
    let pending_modlog = PENDING_MODLOGS
        .write()
        .remove(modlog_id)
        .ok_or("Modlog not found")?;

    // Build and post the final log
    let user_info = format!("[{}:{}]", pending_modlog.username, pending_modlog.roblox_id);
    let note_string = if !pending_modlog.note.is_empty() {
        format!("\nNote: {}", pending_modlog.note)
    } else {
        String::new()
    };

    let log_message = format!(
        "[{}]\n{}\n[{}]{}",
        pending_modlog.action_type, user_info, pending_modlog.reason, note_string
    );

    // Post to the channel
    let channel_id = serenity::all::ChannelId::new(pending_modlog.channel_id);
    channel_id
        .say(ctx, log_message)
        .await?;

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("Modlog posted without images!")
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

/// Posts a modlog with attached images
pub async fn post_modlog_with_images(
    ctx: &Context,
    modlog_id: &str,
    attachments: &[Attachment],
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Retrieve and remove the pending modlog
    let pending_modlog = PENDING_MODLOGS
        .write()
        .remove(modlog_id)
        .ok_or("Modlog not found")?;

    // Build the log message
    let user_info = format!("[{}:{}]", pending_modlog.username, pending_modlog.roblox_id);
    let note_string = if !pending_modlog.note.is_empty() {
        format!("\nNote: {}", pending_modlog.note)
    } else {
        String::new()
    };

    let log_message = format!(
        "[{}]\n{}\n[{}]{}",
        pending_modlog.action_type, user_info, pending_modlog.reason, note_string
    );

    // Post to the channel with images
    let channel_id = serenity::all::ChannelId::new(pending_modlog.channel_id);
    let mut msg_create = serenity::all::CreateMessage::new().content(log_message);

    // Download and attach the actual files
    let mut files = Vec::new();
    for attachment in attachments {
        if is_image_file(&attachment.filename) {
            if let Ok(data) = attachment.download().await {
                files.push(serenity::all::CreateAttachment::bytes(data, &attachment.filename));
            }
        }
    }

    if !files.is_empty() {
        msg_create = msg_create.files(files);
    }

    channel_id.send_message(ctx, msg_create).await?;

    Ok(())
}

/// Helper function to check if a file is an image
fn is_image_file(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
}

/// Cleanup expired modlogs (call periodically)
pub fn cleanup_expired_modlogs() {
    // Note: In a production system, you'd want to add timestamps and clean up old ones
    // For now, this is a placeholder for future enhancement
}
