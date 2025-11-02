use serenity::all::{
    ComponentInteraction, Context, CreateActionRow, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateModal, InputTextStyle, ModalInteraction, UserId,
    Mentionable,
};
use std::error::Error as StdError;

/// Handles the "Create Modlog" button click - opens a modal for input
pub async fn handle_create_log_button(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Parse custom_id: "create_roblox_log:{discord_id}:{username}:{roblox_id}:{command_author_id}"
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();

    if parts.len() < 5 {
        return Err("Invalid button custom_id format".into());
    }

    let discord_id = parts[1];
    let username = parts[2];
    let roblox_id = parts[3];
    let command_author_id: UserId = parts[4].parse().map_err(|_| "Invalid author ID")?;

    // Verify that the person clicking the button is the one who issued the command
    if interaction.user.id != command_author_id {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("⚠️ Only the user who issued the command can create a log.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    }

    // Create modal with fields for log creation
    let modal = CreateModal::new(
        format!("roblox_log_modal:{}:{}:{}", discord_id, username, roblox_id),
        "Create Roblox Moderation Log",
    )
    .components(vec![
        CreateActionRow::InputText(
            CreateInputText::new(InputTextStyle::Short, "Action Type", "action_type")
                .placeholder("Game Ban, Kick, Warn, Server Ban, etc.")
                .required(true),
        ),
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

/// Handles the modal submission - creates and posts the log
pub async fn handle_log_modal_submit(
    ctx: &Context,
    interaction: &ModalInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Parse custom_id: "roblox_log_modal:{discord_id}:{username}:{roblox_id}"
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();

    if parts.len() < 4 {
        return Err("Invalid modal custom_id format".into());
    }

    let discord_id = parts[1];
    let username = parts[2];
    let roblox_id = parts[3];

    // Extract form inputs
    let mut action_type = String::new();
    let mut reason = String::new();
    let mut note = String::new();

    for row in &interaction.data.components {
        for component in &row.components {
            if let serenity::all::ActionRowComponent::InputText(input) = component {
                match input.custom_id.as_str() {
                    "action_type" => action_type = input.value.clone().unwrap_or_default(),
                    "reason" => reason = input.value.clone().unwrap_or_default(),
                    "note" => note = input.value.clone().unwrap_or_default(),
                    _ => {}
                }
            }
        }
    }

    // Build the log message using ron-assistant format
    let user_info = if discord_id != "none" {
        // If we have a Discord ID, include it in the format with mention
        let discord_user_id: UserId = discord_id.parse().map_err(|_| "Invalid Discord ID")?;
        format!("[{}:{} - {}:{}]", discord_user_id.mention(), discord_id, username, roblox_id)
    } else {
        // If no Discord ID, just use Roblox info (same as robloxlog format)
        format!("[{}:{}]", username, roblox_id)
    };

    let note_string = if !note.is_empty() {
        format!("\nNote: {}", note)
    } else {
        String::new()
    };

    // Final log format matching ron-assistant style:
    // [Action Type]
    // [@DiscordMention:DiscordID - Username:RobloxID] or [Username:RobloxID]
    // [Reason]
    // Note: {note}
    let log_message = format!(
        "[{}]\n{}\n[{}]{}",
        action_type, user_info, reason, note_string
    );

    // Post the log in the same channel
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(log_message)
                    .ephemeral(false),
            ),
        )
        .await?;

    Ok(())
}
