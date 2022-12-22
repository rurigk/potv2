# pot
A private discord music bot writen in rust

This bot use poise, serenity and songbird to manage commands and media playback but it uses a custom playlist system

NOTE: This was intended to run on GNU/Linux

## Requeriments
* Rust build tools (Only for building)
* yt-dlp (not youtube-dl or youtube-dlc)

## Rust Setup
Just install rust following the official [Install Rust](https://www.rust-lang.org/tools/install) guide

## Build
NOTE: The discord token is built into the binary for now, this is going to be changed    
NOTE: The youtube api token is built into the binary for now, this is going to be changed    
`TOKEN="DISCORDTOKEN" YOUTUBE_TOKEN="YOUTUBETOKEN" cargo build --release`

## Running
For testing
`TOKEN="DISCORDTOKEN" YOUTUBE_TOKEN="YOUTUBETOKEN" cargo run --release`

For running as standalone just use the binary `potv2` generated inside `target/release`    
`./potv2`

## Discord Commands

/register
/join
/leave
/play
/skip

### Why the name
I asked my friend to give me a bot name and he just said `pot` since the `p` is a upside down `b` and sound similar
