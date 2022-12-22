use std::sync::Arc;
use tokio::{time::{sleep, Duration}, sync::RwLockWriteGuard};

use songbird::{
    Songbird,
    Call, Event, EventContext, EventHandler as VoiceEventHandler, TrackEvent,
};

use async_recursion::async_recursion;
use poise::{serenity_prelude::{Context, GuildId, ChannelId, CacheHttp, Mutex, RwLock}, async_trait};

use crate::{PotPlayInputType, pot::SystemPlaylist};

pub struct TrackEndNotifier {
    ctx: poise::serenity_prelude::Context,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
    handler_lock: Arc<Mutex<Call>>,
    playlist: Arc<RwLock<SystemPlaylist>>,
    manager: Arc<Songbird>
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(_track_list) = ctx {
            if let Some(guild_id) = self.guild_id {
                let mut handler = self.handler_lock.lock().await;
                let mut playlist = self.playlist.write().await;
                
                if consume_and_play_on_end(self, &mut handler, guild_id, &mut playlist).await.is_none() {
                    let _ = self.channel_id.say(&self.ctx.http(), "Queue finished").await;
                    let _ = self.channel_id.say(&self.ctx.http(), "Left voice channel").await;
                    drop(handler);
                    let _ = self.manager.remove(guild_id).await;
                }
            }
        }

        None
    }
}

pub async fn voice_join(ctx: crate::Context<'_>) -> Result<Arc<poise::serenity_prelude::Mutex<Call>>, crate::Error> {
    let guild = ctx.guild().ok_or_else( || Box::new(crate::CommandError("Cannot get Guild".into())))?;
    let guild_id = ctx.guild_id().ok_or_else( || Box::new(crate::CommandError("Cannot get Guild ID".into())))?;

    let msg_channel = ctx.channel_id();

    if let Some(channel_id) = guild.voice_states.get(&ctx.author().id).and_then(|voice_state| voice_state.channel_id) {
        if ctx.data().songbird.get(guild_id).is_none() {
            println!("Songbird join");
            let (call_lock, success) = ctx.data().songbird.join(guild_id, channel_id).await;
            
            if let Err(why) = success {
                Err(Box::new(why))
            } else {
                let mut call = call_lock.lock().await;
                call.add_global_event(
                    Event::Track(TrackEvent::End),
                    TrackEndNotifier {
                        ctx: ctx.discord().clone(),
                        channel_id: msg_channel,
                        guild_id: Some(guild_id),
                        handler_lock: call_lock.clone(),
                        playlist: ctx.data().system_playlist.clone(),
                        manager: ctx.data().songbird.clone()
                    },
                );
                drop(call);
                Ok(call_lock.clone())
            }
        } else {
            Err(Box::new(crate::CommandError("Already joined".into())))
        }
    } else {
        Err(Box::new(crate::CommandError("No channel found".into())))
    }
}

pub async fn voice_leave(ctx: crate::Context<'_>) -> Result<(), crate::Error> {
    let guild_id = ctx.guild_id().ok_or_else( || Box::new(crate::CommandError("Cannot get Guild ID".into())))?;

    if ctx.data().songbird.get(guild_id).is_some() {
        let mut playlist = ctx.data().system_playlist.write().await;

        playlist.clear(guild_id);
        playlist.set_status(guild_id, false);

        if let Err(e) = ctx.data().songbird.remove(guild_id).await {
            let _ = ctx.channel_id().say(&ctx.discord(), format!("Failed: {:?}", e)).await;
            return Err( Box::new(crate::CommandError( format!("Failed: {:?}", e) )) )
        }

        Ok(())
    } else {
        Err( Box::new(crate::CommandError("Not in a voice channel".into())) )
    }
}

pub async fn song_skip(ctx: crate::Context<'_>) -> Result<String, crate::Error> {
    let guild_id = ctx.guild_id().ok_or_else( || Box::new(crate::CommandError("Cannot get Guild ID".into())))?;

    if let Some(handler_lock) = ctx.data().songbird.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        handler.stop();

        let mut playlist = ctx.data().system_playlist.write().await;

        if playlist.is_playing(guild_id) {
            if consume_and_play(ctx.channel_id(), ctx.discord(), &mut playlist, guild_id, &mut handler).await.is_none() {
                drop(handler);
                let _ = ctx.data().songbird.remove(guild_id).await;
                Ok("Queue ended".into())
            } else {
                Ok("Song skipped".into())
            }
        } else {
            Ok("Nothing to play".into())
        }
    } else {
        Err( Box::new(crate::CommandError("Not in a voice channel".into())) )
    }
}

#[async_recursion]
async fn consume_and_play(channel_id: ChannelId, http: &Context, playlist: &mut SystemPlaylist, guild_id: GuildId, call: &mut tokio::sync::MutexGuard<'_, Call>) -> Option<()> {
    // Try to consume a item from the playlist
    match playlist.consume(guild_id) {
        Some(playlist_item) => {
            // If we found a PlaylistItem available we change the playlist status to playing
            playlist.set_status(guild_id, true);
            
            // Then we try to get the mefia file
            match playlist.get_media_stream(&playlist_item).await {
                Ok(source) => {
                    println!("{:?}", source);
                    // Send message to channel
                    let _ = channel_id.say(&http, format!("Playing now {}", playlist_item.title)).await;

                    // Play the source
                    let _ = call.play_only_source(source);
                    Some(())
                },
                Err(err) => {
                    println!("{:?}", err);
                    // Set status to not playing
                    playlist.set_status(guild_id, false);
                    // Send message of error
                    let _ = channel_id.say(&http, format!("Cannot play {}", playlist_item.title)).await;
                    // Try again
                    consume_and_play(channel_id, http, playlist, guild_id, call).await
                }
            }
        },
        None => {
            // No more items in playlist
            let _ = channel_id.say(&http, "Queue finished").await;
            // Set status to not playing
            playlist.set_status(guild_id, false);
            None
        }
    }
}

#[async_recursion]
pub async fn consume_and_play_on_end (slf: &TrackEndNotifier, handler: &mut tokio::sync::MutexGuard<'_, Call>, guild_id: GuildId, playlist: &mut RwLockWriteGuard<SystemPlaylist>) -> Option<()> {
    match playlist.consume(guild_id) {
        Some(item) => {
            println!("consumed");
            match playlist.get_media_stream(&item).await {
                Ok(source) => {
                    println!("media getted");

                    let _ = slf.channel_id.say(&slf.ctx.http(), format!("Playing now {}", item.title)).await;
                    handler.play_only_source(source);
                    Some(())
                },
                Err(err) => {
                    println!("{:?}", err);
                    println!("media not getted");
                    let _ = slf.channel_id.say(&slf.ctx.http(), format!("Cannot play {}", item.title)).await;
                    consume_and_play_on_end(slf, handler, guild_id, playlist).await
                },
            }
        },
        None => {
            playlist.set_status(guild_id, false);
            None
        },
    }
}

#[poise::command(slash_command, guild_only)]
pub async fn join(
    ctx: crate::Context<'_>,
) -> Result<(), crate::Error> {
    match voice_join(ctx).await {
        Ok(_) => { let _ = ctx.send(|r| r.content("Joined")).await;},
        Err(err) => {
            println!("voice join error {}", err);
            let _ = ctx.send(|r| r.content("Cannot join")).await;
        },
    };

    Ok(())
}

#[poise::command(slash_command, guild_only)]
pub async fn leave(
    ctx: crate::Context<'_>,
) -> Result<(), crate::Error> {
    match voice_leave(ctx).await {
        Ok(_) => { let _ = ctx.send(|r| r.content("Left voice channel")).await;},
        Err(err) => {
            let _ = ctx.send(|r| r.content(err.to_string())).await;
        },
    };

    Ok(())
}

#[poise::command(slash_command, guild_only)]
pub async fn play(
    ctx: crate::Context<'_>,
    #[description = "Search a song or use a url to a song"]
    song: String,
) -> Result<(), crate::Error> {
    use url::{Url};

    let src = song;
    
    // Get pot input type from src
    let input = match Url::parse(&src) {
        Ok(url_parsed) => PotPlayInputType::Url(url_parsed),
        Err(_) => PotPlayInputType::Search(src)
    };

    let guild_id = ctx.guild_id().ok_or_else( || Box::new(crate::CommandError("Cannot get Guild ID".into())))?;

    let songbird = ctx.data().songbird.clone();
    let mut playlist = ctx.data().system_playlist.write().await;

    match voice_join(ctx).await {
        Ok(_) => { 
            let _ = ctx.send(|r| r.content("Joined")).await;
            sleep(Duration::from_millis(500)).await;
        },
        Err(err) => {println!("voice join error {}", err);},
    };

    if let Some(call_mutex) = ctx.data().songbird.get(guild_id) {
        let mut call = call_mutex.lock().await;
        
        match playlist.add(guild_id, input).await {
            Ok(items_added) => {
                if items_added > 1 {
                    let _ = ctx.channel_id().say(&ctx.discord(), format!("{} songs added", items_added)).await;
                } else {
                    let _ = ctx.channel_id().say(&ctx.discord(), "1 song added").await;
                }

                if !playlist.is_playing(guild_id) && consume_and_play(ctx.channel_id(), ctx.discord(), &mut playlist, guild_id, &mut call).await.is_none(){
                    drop(call);
                    let _ = songbird.remove(guild_id).await;
                    let _ = ctx.channel_id().say(&ctx.discord(), "Left voice channel").await;
                }
            },
            Err(_err) => {
                let _ = ctx.channel_id().say(&ctx.discord(), "Error adding to the playlist").await;
            }
        }
    }

    Ok(())
}

#[poise::command(slash_command, guild_only)]
pub async fn skip(
    ctx: crate::Context<'_>,
) -> Result<(), crate::Error> {
    match song_skip(ctx).await {
        Ok(msg) => { 
            let _ = ctx.send(|r| r.content(msg)).await;
        },
        Err(err) => {
            let _ = ctx.send(|r| r.content(err.to_string())).await;
        },
    };

    Ok(())
}