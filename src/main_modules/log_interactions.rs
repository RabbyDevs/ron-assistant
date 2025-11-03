use poise::SlashArgument;
use serenity::all::{
    ComponentInteraction, ComponentInteractionDataKind, Context, CreateActionRow, CreateInputText,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateModal, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption, InputTextStyle, ModalInteraction,
};
use std::error::Error as StdError;

use crate::commands::log_module::roblox_log::RobloxInfTypes;

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

/// Handles the modal submission - creates and posts the log
pub async fn handle_log_modal_submit(
    ctx: &Context,
    interaction: &ModalInteraction,
) -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Parse custom_id: "roblox_log_modal:{discord_id}:{username}:{roblox_id}"
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();

    if parts.len() < 3 {
        return Err("Invalid modal custom_id format".into());
    }

    let username = parts[1];
    let roblox_id = parts[2];

    // Extract form inputs
    let mut action_type = parts[3];
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

    // Build the log message using format
    let user_info = format!("[{}:{}]", username, roblox_id);

    let note_string = if !note.is_empty() {
        format!("\nNote: {}", note)
    } else {
        String::new()
    };

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
