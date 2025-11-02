use crate::Data;
use crate::CONFIG;
use super::{Context, Error};
use poise::Modal;

#[poise::command(slash_command, prefix_command, 
    subcommands("edit", "delete", "publish", "list", "clear_all"),
    subcommand_required)]
/// Command for managing policies
pub async fn guide(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[derive(Debug, Modal)]
#[name = "Guide Editor:"] // Struct name by default
struct EditModal {
    order: String,
    #[paragraph]
    content: String,
}

async fn has_required_role(ctx: &poise::ApplicationContext<'_, Data, Error>, author: &User) -> bool {
    let role_list = CONFIG.main.admin_role_ids;
    let mut has_role = false;
    for role in role_list {
        if author.has_role(ctx.http(), GuildId::new(CONFIG.main.guild_id.parse().unwrap()), RoleId::new(role.parse().unwrap())).await.unwrap() {has_role = true}
    }

    has_role
}

#[poise::command(slash_command)]
/// Edit an existing guide
pub async fn edit(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    #[description = "Guide internal name"] internal_name: String,
) -> Result<(), Error> {
    if !(has_required_role(&ctx, ctx.author()).await) {
        ctx.say("You must be an administrator in Rise of Nations to use this command.").await?;
        return Ok(())
    }
    let guide_system = &ctx.data().guide_system;

    let data = EditModal::execute(ctx).await?;
    let data = data.unwrap();
    // Edit the guide
    guide_system.edit(&internal_name, data.content, data.order.parse::<u64>().unwrap()).unwrap();
    
    // Notify the user
    ctx.say(format!("Guide '{}' updated and changes cached.", internal_name)).await?;
    Ok(())
}

#[poise::command(slash_command)]
/// Delete an existing guide
pub async fn delete(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    #[description = "Guide internal name"] internal_name: String,
) -> Result<(), Error> {
    if !(has_required_role(&ctx, ctx.author()).await) {
        ctx.say("You must be an administrator in Rise of Nations to use this command.").await?;
        return Ok(())
    }
    let guide_system = &ctx.data().guide_system;
    guide_system.remove(&internal_name).unwrap();
    
    ctx.say(format!("Guide '{}' deleted and changes cached.", internal_name)).await?;
    Ok(())
}

#[poise::command(slash_command)]
/// Publish all cached changes
pub async fn publish(
    ctx: poise::ApplicationContext<'_, Data, Error>
) -> Result<(), Error> {
    if !(has_required_role(&ctx, ctx.author()).await) {
        ctx.say("You must be an administrator in Rise of Nations to use this command.").await?;
        return Ok(())
    }
    let guide_system = &ctx.data().guide_system;
    ctx.say("Guide cached changes applying.".to_string()).await?;
    guide_system.update_guide(&ctx.serenity_context().clone()).await.unwrap();
    Ok(())
}

#[poise::command(slash_command)]
/// List all policies and their internal names
pub async fn list(
    ctx: poise::ApplicationContext<'_, Data, Error>
) -> Result<(), Error> {
    if !(has_required_role(&ctx, ctx.author()).await) {
        ctx.say("You must be an administrator in Rise of Nations to use this command.").await?;
        return Ok(())
    }
    let guide_system = &ctx.data().guide_system;
    let policies = guide_system.list_policies_internal_names().unwrap();
    let mut guide_list_string = String::from("Current Guide Internal Names:");

    for (internal_name, entry) in policies.iter() {
        guide_list_string.push_str(format!("\n{} - Order: {}", internal_name, entry.order).as_str());
    }

    ctx.say(guide_list_string).await?;

    Ok(())
}

use poise::serenity_prelude as serenity;
use ::serenity::all::GuildId;
use ::serenity::all::RoleId;
use ::serenity::all::User;

#[poise::command(slash_command)]
/// Clear all policies
pub async fn clear_all(
    ctx: poise::ApplicationContext<'_, Data, Error>
) -> Result<(), Error> {
    if !(has_required_role(&ctx, ctx.author()).await) {
        ctx.say("You must be an administrator in Rise of Nations to use this command.").await?;
        return Ok(())
    }
    let guide_system = &ctx.data().guide_system;
    let uuid_yes = ctx.id();
    let uuid_no = uuid::Uuid::new_v4();

    let reply = {
        let components = vec![serenity::CreateActionRow::Buttons(vec![
            serenity::CreateButton::new(format!("{uuid_yes}"))
                .style(serenity::ButtonStyle::Danger)
                .label("Yes, Clear All"),
            serenity::CreateButton::new(format!("{uuid_no}"))
                .style(serenity::ButtonStyle::Secondary)
                .label("No, Cancel"),
        ])];
        poise::CreateReply::default()
            .content("Are you sure you want to clear all policies? This action cannot be undone.")
            .components(components)
    };

    ctx.send(reply).await?;

    if let Some(mci) = serenity::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .channel_id(ctx.channel_id())
        .timeout(std::time::Duration::from_secs(60))
        .filter(move |mci| mci.data.custom_id == uuid_yes.to_string() || mci.data.custom_id == uuid_no.to_string())
        .await
    {
        if mci.data.custom_id == uuid_yes.to_string() {
            // Actually clear all policies
            match guide_system.clear_all() {
                Ok(_) => {
                    mci.create_response(
                        ctx,
                        serenity::CreateInteractionResponse::UpdateMessage(
                            serenity::CreateInteractionResponseMessage::new()
                                .content("All policies have been cleared successfully.")
                                .components(vec![]),
                        ),
                    )
                    .await?;
                },
                Err(e) => {
                    mci.create_response(
                        ctx,
                        serenity::CreateInteractionResponse::UpdateMessage(
                            serenity::CreateInteractionResponseMessage::new()
                                .content(format!("An error occurred while clearing policies: {:?}", e))
                                .components(vec![]),
                        ),
                    )
                    .await?;
                }
            }
        } else {
            // User clicked "No"
            mci.create_response(
                ctx,
                serenity::CreateInteractionResponse::UpdateMessage(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Operation cancelled. No policies were cleared.")
                        .components(vec![]),
                ),
            )
            .await?;
        }
    } else {
        // Timeout occurred, update the message
        ctx.channel_id()
            .say(
                &ctx.serenity_context(),
                "Operation timed out. No policies were cleared.",
            )
            .await?;
    }

    Ok(())
}