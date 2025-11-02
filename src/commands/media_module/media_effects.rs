use std::{fmt, io::Write};
use serenity::all::{Attachment, EditMessage};
use uuid::Uuid;

use super::{Context, Error, apply_mask};

#[derive(Debug, poise::ChoiceParameter)]
pub enum SpeechBubbleOverlays {
    #[name = "esm Bot Style"]
    EsmBotStyle,
    #[name = "RON Bot Style"]
    RONBotStyle
}

impl fmt::Display for SpeechBubbleOverlays {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpeechBubbleOverlays::EsmBotStyle => write!(f, "EsmBotStyle"),
            SpeechBubbleOverlays::RONBotStyle => write!(f, "RONBotStyle"),
        }
    }
}

use std::{fs, path::Path};

#[poise::command(slash_command, prefix_command, subcommand_required, subcommands("speechbubble"))]
pub async fn media(_: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(prefix_command, slash_command)]
pub async fn speechbubble(
    ctx: Context<'_>,
    #[description = "Attachment for command."] attachment: Attachment,
    #[description = "Overlay type, defaults to esm bot style."] style: Option<SpeechBubbleOverlays>,
    #[description = "A bit technical, but what should the height of the overlay be divided by? In 0.0-1.0."] height_float: Option<f32>,
    #[description = "Should the speech bubble be flipped horizontally?"] flip: Option<bool>,
    #[description = "Should the speech bubble be transparent? By default set to true if image."] transparent: Option<bool>,
    #[description = "Should the speech bubble be, if its an image, be converted to gif? False for yes, true for no."] no_force_gif: Option<bool>,
) -> Result<(), Error> {
    let msg = ctx.say("Adding speechbubble...").await?;
    let style = style.unwrap_or(SpeechBubbleOverlays::EsmBotStyle);

    let response = ctx.data().reqwest_client.get(&attachment.url).send().await?;
    let bytes = response.bytes().await?;

    let input_extension = Path::new(&attachment.filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or("Invalid file extension")?
        .to_lowercase();

    let input_path = match input_extension.as_str() {
        "png" | "jpg" | "jpeg" | "mp4" | "mov" => format!("./.tmp/input_{}.{}", Uuid::new_v4(), input_extension),
        _ => return Err("Unsupported file format".into()),
    };

    let mut file = fs::File::create(&input_path)?;
    file.write_all(&bytes)?;
    
    let overlay_path = format!("./.default_masks/{}.png", style);

    if !Path::new(&overlay_path).exists() {
        return Err(format!("Overlay file not found: {}", overlay_path).into());
    }

    let output_path = match apply_mask(input_path.clone(), &overlay_path, flip.unwrap_or(false), height_float.unwrap_or(0.2), transparent.unwrap_or(true), no_force_gif.unwrap_or(false)) {
        Ok(str) => {
            str
        },
        Err(err) => {
            return Err(err.into());
        }
    };

    let file = serenity::all::CreateAttachment::path(&output_path).await?;
    msg.into_message().await?.edit(ctx.http(), EditMessage::new().new_attachment(file).content("Done!")).await?;

    fs::remove_file(&input_path)?;
    fs::remove_file(&output_path)?;

    Ok(())
}