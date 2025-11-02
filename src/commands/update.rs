use std::process::{Command, Stdio};

use super::{Context, Error};

#[poise::command(slash_command, prefix_command)]
/// Command for updating the bot.
pub async fn update(
    ctx: Context<'_>,
) -> Result<(), Error> {
    ctx.reply("Updating!").await?;
    let child = Command::new("sh")
        .arg("-c")
        .arg("/root/rabby-stuff/ron-assista-bot/update.sh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    println!("Script started with PID: {}", child.id());
    
    // Immediately detach by not calling .wait()
    Ok(())
}