mod helpers;
mod commands;
mod pot;
mod yt;

use std::{sync::Arc, fmt};
use poise::{serenity_prelude::{self as serenity, RwLock}};

use crate::{pot::{SystemPlaylist, PotPlayInputType}};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Debug)]
struct CommandError(String);

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There is an error: {}", self.0)
    }
}

impl std::error::Error for CommandError {}

// User data, which is stored and accessible in all command invocations
pub struct Data {
    pub songbird: Arc<songbird::Songbird>,
    pub system_playlist: Arc<RwLock<SystemPlaylist>>
}

#[poise::command(prefix_command)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[poise::command(slash_command, guild_only)]
pub async fn ping(
    ctx: crate::Context<'_>,
) -> Result<(), crate::Error> {
    let _ = ctx.send(|r| r.content("pong")).await;

    Ok(())
}

#[tokio::main]
async fn main() {
    // Setup dir structure
    match helpers::setup_system() {
        Ok(_) => println!("Directories setup complete"),
        Err(err) => {
            panic!("{:?}", err);
        },
    }

    let songbird = songbird::Songbird::serenity();
    let system_playlist = Arc::new(RwLock::new(SystemPlaylist::new()));

    let data = Data {
        songbird: songbird.clone(),
        system_playlist: system_playlist.clone()
    };

    // Start poise framework
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                edit_tracker: Some(poise::EditTracker::for_timespan(std::time::Duration::from_secs(3600))),
                case_insensitive_commands: true,
                ..Default::default()
            },
            commands: vec![
                register(),
                ping(),
                commands::shitpost_reactions::shut(),
                commands::shitpost_reactions::pato(),
                commands::voice_commands::join(),
                commands::voice_commands::play(),
                commands::voice_commands::skip(),
                commands::voice_commands::leave(),
            ],
            ..Default::default()
        })
        // .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .token(option_env!("DISCORD_TOKEN").expect("No DISCORD_TOKEN set on compile time"))
        .client_settings(|f| f.voice_manager_arc(songbird))
        .intents(serenity::GatewayIntents::GUILDS
            | serenity::GatewayIntents::GUILD_MESSAGES
            | serenity::GatewayIntents::DIRECT_MESSAGES
            | serenity::GatewayIntents::GUILD_VOICE_STATES
            | serenity::GatewayIntents::MESSAGE_CONTENT)
        .user_data_setup(move |_ctx, _ready, _framework| Box::pin(async move { 
            Ok(data) 
        }));

    framework.run().await.unwrap();
}