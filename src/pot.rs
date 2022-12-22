use anyhow::{anyhow};
use serde::{Deserialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, BufRead};
use std::path::Path;
use std::process::ChildStdout;
use std::{
    io::{Read},
    process::{Command, Stdio},
};

use poise::{serenity_prelude::{ GuildId, TypeMapKey}};


#[cfg(not(feature = "tokio-02-marker"))]
use tokio::{task};
#[cfg(feature = "tokio-02-marker")]
use tokio_compat::{task};

use crate::helpers;
use crate::yt::YoutubeResult;

const YOUTUBE_DL_COMMAND: &str = "yt-dlp";


pub struct SystemPlaylistData;

impl TypeMapKey for SystemPlaylistData {
     type Value = SystemPlaylist;
}

pub struct SystemPlaylist {
    guilds_playlists: HashMap<u64, Vec<PlaylistItem>>,
    guilds_playing: HashMap<u64, bool>
}

pub enum PotPlayInputType {
    Url(url::Url),
    Search(String)
}

impl PotPlayInputType {
    fn is_url(&self) -> bool {
        matches!(*self, Self::Url(_))
    }
}

enum YoutubeUrlType {
    Video(String),
    Playlist(String),
    None
}
fn youtube_url_extractor (url: &url::Url) -> YoutubeUrlType {
    match url.host_str() {
        Some(url_str) => {
            if url_str.ends_with("youtube.com") || url_str.ends_with("youtu.be") {
                let query = query_pairs_to_hashmap(url);

                if query.contains_key("list") {
                    YoutubeUrlType::Playlist(query.get("list").unwrap().to_owned())
                } else if query.contains_key("v") {
                    YoutubeUrlType::Video(query.get("v").unwrap().to_owned())
                } else {
                    YoutubeUrlType::None
                }
            } else {
                YoutubeUrlType::None
            }
        },
        None => YoutubeUrlType::None,
    }
}

fn query_pairs_to_hashmap (url: &url::Url) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for (key, value) in url.query_pairs() {
        let qkey = key.to_string();
        let qvalue = value.to_string();
        map.entry(qkey).or_insert(qvalue);
    }
    map
}

fn youtube_result_to_playlist_items (yt_result: YoutubeResult) -> Vec<PlaylistItem> {
    if let YoutubeResult::Ok(response) = yt_result {
        response.items.into_iter().filter_map(|item| {
            if let Some(resource_id) = item.snippet.resourceId {
                Some(PlaylistItem {
                    original_url: format!("https://www.youtube.com/watch?v={}", &resource_id.videoId),
                    id: resource_id.videoId,
                    title: item.snippet.title,
                    extractor: "youtube".to_string(),
                    thumbnail: None,
                    duration: None,
                    playlist_id: None,
                    webpage_url: None,
                    is_live: None,
                    was_live: None,
                })
            } else {
                None
            }
        }).collect()
    } else {
        Vec::new()
    }
}

impl SystemPlaylist {
    pub fn new () -> Self {
        Self {
            guilds_playlists: HashMap::new(),
            guilds_playing: HashMap::new()
        }
    }

    pub fn set_status (&mut self, guild: GuildId, is_playing: bool) {
        let guild_id = guild.as_u64();
        if self.guilds_playing.contains_key(guild_id) {
            let guild_playlist_status = self.guilds_playing.get_mut(guild_id).unwrap();
            *guild_playlist_status = is_playing;

            println!("status set :{}", guild_playlist_status);
        } else {
            self.guilds_playing.insert(guild_id.to_owned(), is_playing);
            println!("status set :{}", is_playing);
        }
    }

    pub fn is_playing (&self, guild: GuildId) -> bool {
        let guild_id = guild.as_u64();
        if self.guilds_playing.contains_key(guild_id) {
            *self.guilds_playing.get(guild_id).unwrap()
        } else {
            false
        }
    }

    /// Consumes and return a item from the the guild playlist removing the item
    pub fn consume(&mut self, guild: GuildId) -> Option<PlaylistItem> {
        if self.guilds_playlists.contains_key(guild.as_u64()) { // Guild playlist already exist
            let guild_playlist = self.guilds_playlists.get_mut(guild.as_u64()).unwrap();
            
            if guild_playlist.is_empty() {
                None
            } else {
                Some(guild_playlist.remove(0))
            }
        } else { // The guild playlist is not currently in the system
            None
        }
    }

    /// Try to fetch a playlist or a single media item and add it to the guild playlist
    pub async fn add(&mut self, guild: GuildId, input: PotPlayInputType) -> anyhow::Result<usize> {
        use crate::yt::YoutubeAPI;

        let token_env = option_env!("YOUTUBE_TOKEN");
        let token = match token_env {
            Some(token_env) => String::from(token_env),
            None => panic!("No YOUTUBE_TOKEN set on compile time"),
        };

        let api = YoutubeAPI::new(&token);

        let is_url = input.is_url();

        let playlist_result = if let PotPlayInputType::Url(url) = input {
            // Detect if the url is a youtube url
            let extractor_result = youtube_url_extractor (&url);
            
            if let YoutubeUrlType::Playlist(playlist_id) = extractor_result {
                Ok(youtube_result_to_playlist_items(api.playlist(&playlist_id).await))
            } else if let YoutubeUrlType::Video(video_id) = extractor_result {
                Ok(youtube_result_to_playlist_items(api.video(&video_id).await))
            } else {
                Self::get_playlist(url.as_str()).await
            }
        } else if let PotPlayInputType::Search(query) = input {
            Self::get_playlist(&format!("ytsearch1:{}", query)).await
        } else {
            Ok(Vec::new())
        };

        match playlist_result {
            Ok(mut playlist) => {

                if self.guilds_playlists.contains_key(guild.as_u64()) { // Guild playlist already exist
                    let guild_playlist = self.guilds_playlists.get_mut(guild.as_u64()).unwrap();

                    if is_url {
                        let playlist_size = playlist.len();
                        guild_playlist.append(&mut playlist);
                        Ok(playlist_size)
                    } else {
                        match playlist.get(0) {
                            Some(item) => {
                                guild_playlist.push(item.to_owned());
                                Ok(1)
                            },
                            None => Ok(0),
                        }
                        
                    }
                } else { // Guild playlist not exist creating one
                    let playlist_size = playlist.len();

                    if is_url {
                        self.guilds_playlists.insert(*guild.as_u64(), playlist);
                        Ok(playlist_size)
                    } else if playlist_size > 0 {
                        self.guilds_playlists.insert(*guild.as_u64(), Vec::new());
                        let guild_playlist = self.guilds_playlists.get_mut(guild.as_u64()).unwrap();
                        guild_playlist.push(playlist.get(0).unwrap().to_owned());
                        Ok(1)
                    } else {
                        Ok(0)
                    }
                    
                }
            },
            Err(err) => Err(err),
        }
    }

    /// Remove all items from the playlist and returns true if the playlist is cleared of false if the guild has no playlist
    pub fn clear(&mut self, guild: GuildId) -> bool{
        if self.guilds_playlists.contains_key(guild.as_u64()) { // Guild playlist already exist
            let guild_playlist = self.guilds_playlists.get_mut(guild.as_u64()).unwrap();
            guild_playlist.clear();

            true
        } else { // The guild playlist is not currently in the system
            false
        }
    }

    /// Fetch playlist with yt-dlp and parse the result
    async fn get_playlist (url: &str) -> anyhow::Result<Vec<PlaylistItem>> {
        let ytdl_args = [
            "-j",
            "-f",
            "webm[abr>0]/bestaudio/best",
            "-R",
            "infinite",
            "--yes-playlist",
            "--ignore-config",
            "--no-warnings",
            url,
            "-o",
            "-",
        ];

        let mut ytdlp_child = Command::new(YOUTUBE_DL_COMMAND)
        .args(ytdl_args)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

        // This rigmarole is required due to the inner synchronous reading context.
        let stderr = ytdlp_child.stderr.take();

        let (_returned_stderr, value) = task::spawn_blocking(move || {
            let mut child_stderr = stderr.unwrap();

            let mut output_data: String = String::new();
            let _ = child_stderr.read_to_string(&mut output_data);

            (child_stderr, output_data)
        })
        .await?;

        let _ = ytdlp_child.wait();

        let jsons: Vec<&str> = value.split('\n').collect();

        let items: Vec<PlaylistItem> = jsons.iter().filter_map(|json_str| {
            match serde_json::from_str::<PlaylistItem>(json_str) {
                Ok(item) => Some(item),
                Err(_) => None,
            }
        }).collect();

        Ok(items)
    }

    pub async fn get_media (&self, item: &PlaylistItem) -> Option<songbird::input::Restartable> {
        let _ = helpers::graceful_mkdir("data/cache");
        let fpath = format!("data/cache/media/{}/{}", item.extractor, item.id);
        let path = Path::new(&fpath);

        let file_path = if Self::check_file(path) {
            println!("Loaded from cache");
            Some(path.to_str().unwrap().to_string())
        } else {
            println!("Loaded from ytdl");
            let path_str = path.to_str().unwrap();

            Self::ytdlp_download(path_str, &item.original_url).await;
    
            if Self::check_file(path) {
                Some(path_str.to_string())
            } else {
                None
            }
        };

        match file_path {
            Some(file_path) => {
                match songbird::input::Restartable::ffmpeg(file_path, false).await {
                    Ok(source) => Some(source),
                    Err(_why) => None,
                }
            },
            None => None,
        }
    }

    pub async fn get_media_stream(&self, item: &PlaylistItem) -> anyhow::Result<songbird::input::Input> {
        let ytdlp_child = Self::ytdlp_stream(&item.original_url).await?;
        let input = Self::ffmpeg_to_input(ytdlp_child).await?;
        Ok(input)
    }

    pub async fn ytdlp_download(path_str: &str, item_original_url: &str) {
        let ytdl_args = [
            "--print-json",
            "-f",
            "webm[abr>0]/bestaudio/best",
            "-R",
            "infinite",
            "--no-playlist",
            "--ignore-config",
            "--no-warnings",
            item_original_url,
            "-o",
            path_str,
        ];

        let mut yt_dlp = Command::new(YOUTUBE_DL_COMMAND)
            .args(ytdl_args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .spawn().expect("ytdlp failed to execute");

        let _ = yt_dlp.wait();
    }

    // Calls yt-dlp and gets the file data from stdout
    pub async fn ytdlp_stream(item_original_url: &str) -> anyhow::Result<std::process::Child> {
        let ytdl_args = [
            "--print-json",
            "-f",
            "webm[abr>0]/bestaudio/best",
            "-R",
            "infinite",
            "--no-playlist",
            "--ignore-config",
            "--no-warnings",
            item_original_url,
            "-o",
            "-",
        ];

        // let log = fs::File::create("debug.txt").expect("failed to open log");

        let mut yt_dlp = Command::new(YOUTUBE_DL_COMMAND)
            .args(ytdl_args)
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn().expect("ytdlp failed to execute");

        // This rigmarole is required due to the inner synchronous reading context.
        let stderr = yt_dlp.stderr.take();
        let returned_stderr = task::spawn_blocking(move || {
            let mut children_stderr = stderr.unwrap();

            let mut reader = BufReader::new(children_stderr.by_ref());

            let mut o_vec = vec![];
            let _ = reader.read_until(0xA, &mut o_vec);

            children_stderr
        })
        .await?;

        yt_dlp.stderr = Some(returned_stderr);

        Ok(yt_dlp)
    }

    pub async fn ffmpeg_to_input(mut input: std::process::Child) -> anyhow::Result<songbird::input::Input>{
        let taken_stdout = input.stdout.take().ok_or_else(|| anyhow!("Failed to take children stdout"))?;

        let ffmpeg_args = [
            "-f",
            "s16le",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-acodec",
            "pcm_f32le",
            "-",
        ];

        let ffmpeg = Command::new("ffmpeg")
            .arg("-i")
            .arg("-")
            .args(ffmpeg_args)
            .stdin(taken_stdout)
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?;

        Ok(songbird::input::Input::new(
            true,
            songbird::input::children_to_reader::<f32>(vec![input, ffmpeg]),
            songbird::input::Codec::FloatPcm,
            songbird::input::Container::Raw,
            Default::default(),
        ))
    }

    pub async fn save_stdout(input: ChildStdout) -> anyhow::Result<()>{
        let tee_args = [
            "debug.txt",
        ];

        let mut _tee = Command::new("tee")
            .args(tee_args)
            .stdin(input)
            .stderr(Stdio::inherit())
            .stdout(Stdio::inherit())
            .spawn()?;
        
            // let _ = _tee.wait();

        Ok(())
    }

    fn check_file (path: &Path) -> bool {
        match fs::metadata(path) {
            Ok(attributes) => {
                !attributes.is_dir()
            },
            Err(_) => {
                false
            }
        }
    }
}

impl Default for SystemPlaylist {
    fn default() -> Self {
        Self::new()
    }
}

    
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct PlaylistItem {
    pub id: String,
    pub title: String,
    pub original_url: String,
    pub extractor: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f32>,
    pub playlist_id: Option<String>,
    pub webpage_url: Option<String>,
    pub is_live: Option<bool>,
    pub was_live: Option<bool>
}