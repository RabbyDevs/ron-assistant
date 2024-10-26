use std::io::Write;
use ::serenity::all::Attachment;
use serenity::all::CreateMessage;
use uuid::Uuid;

use super::{Context, Error, video_format_changer, image_to_png_converter, png_to_gif_converter, video_to_gif_converter, QualityPreset};

#[poise::command(slash_command, prefix_command)]
/// Command for converting any video/display format to a gif, dynamically, for free.
pub async fn gif(
    ctx: Context<'_>,
    #[description = "Attachment for command."] attachment: Attachment,
    #[description = "Quality Preset for the command."] quality_preset: Option<QualityPreset>
) -> Result<(), Error> {
    ctx.reply("Converting attachment into gif, this may take a while!").await.unwrap();
    let quality_preset = quality_preset.unwrap_or(QualityPreset::HighQuality);
    
    let content_type = match attachment.content_type {
        Some(ct) => ct,
        None => {
            ctx.say("No content type found.").await?;
            return Ok(());
        }
    };

    let main_input_filename = format!("./.tmp/main_input_{}..tmp", Uuid::new_v4());
    let response = ctx.data().reqwest_client.get(&attachment.url).send().await?;
    let bytes = response.bytes().await?;
    let mut file = std::fs::File::create(&main_input_filename)?;
    file.write_all(&bytes)?;

    let result = if content_type.contains("video/") {
        convert_video(&content_type, main_input_filename, quality_preset).await
    } else if content_type.contains("image") {
        convert_image(&content_type, main_input_filename, quality_preset).await
    } else {
        std::fs::remove_file(&main_input_filename)?;
        Err(Error::from("Unsupported file type"))
    };

    match result {
        Ok(output_filename) => {
            let send_result = async {
                let file = serenity::all::CreateAttachment::path(&output_filename).await?;
                let builder = CreateMessage::new().content("Done!");
                ctx.channel_id().send_files(&ctx.http(), vec![file], builder).await?;
                Ok::<_, Error>(())
            }.await;

            std::fs::remove_file(output_filename).unwrap();

            if let Err(e) = send_result {
                ctx.say(format!("Failed to send the converted file: {}", e)).await?;
            }
        },
        Err(e) => {
            ctx.say(format!("Failed to convert the file: {}", e)).await?;
        }
    }

    Ok(())
}

async fn convert_video(content_type: &str, input: String, quality_preset: QualityPreset) -> Result<String, Error> {
    let output_filename = format!("./.tmp/output_{}.gif", Uuid::new_v4());
    
    let result = if content_type != "video/mp4" {
        let mp4_output_filename = format!("./.tmp/output_{}.mp4", Uuid::new_v4());
        let output = video_format_changer(&input, &mp4_output_filename);
        std::fs::remove_file(&input).ok();
        
        if output.status.success() {
            let gif_output = video_to_gif_converter(&mp4_output_filename, &output_filename, quality_preset);
            std::fs::remove_file(&mp4_output_filename).ok();
            handle_command_output(gif_output, output_filename.clone())
        } else {
            print!("{:#?}", output);
            std::fs::remove_file(&mp4_output_filename).ok();
            Err(Error::from("Failed to convert video format"))
        }
    } else {
        let output = video_to_gif_converter(&input, &output_filename, quality_preset);
        std::fs::remove_file(&input).ok();
        handle_command_output(output, output_filename.clone())
    };

    if result.is_err() {
        std::fs::remove_file(&output_filename).ok();
    }
    
    result
}

async fn convert_image(content_type: &str, input: String, quality_preset: QualityPreset) -> Result<String, Error> {
    let output_filename = format!("./.tmp/output_{}.gif", Uuid::new_v4());
    
    if content_type != "image/png" {
        let png_output_filename = format!("./.tmp/output_{}.png", Uuid::new_v4());
        let output = image_to_png_converter(&input, &png_output_filename);
        std::fs::remove_file(&input).unwrap();
        
        if output.status.success() {
            let gif_output: Result<(), std::io::Error> = png_to_gif_converter(&png_output_filename, &output_filename, quality_preset);
            std::fs::remove_file(&png_output_filename).unwrap();
            handle_command_output(gif_output, output_filename.clone())
        } else {
            std::fs::remove_file(&png_output_filename).unwrap();
            Err(Error::from("Failed to convert image to PNG"))
        }
    } else {
        let output = png_to_gif_converter(&input, &output_filename, quality_preset);
        std::fs::remove_file(&input).unwrap();
        handle_command_output(output, output_filename.clone())
    }
}

fn handle_command_output(output: Result<(), std::io::Error>, output_filename: String) -> Result<String, Error> {
    if output.is_ok() {
        Ok(output_filename)
    } else {
        Err(Error::from(format!(
            "Conversion failed: {}",
            &output.err().unwrap().to_string()
        )))
    }
}