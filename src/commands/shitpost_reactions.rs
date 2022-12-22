use poise::{self};
use crate::{Error, Context};

#[poise::command(slash_command)]
pub async fn shut(
    ctx: Context<'_>
) -> Result<(), Error> {
    let _ = ctx.send(|r| {
        r.embed(|e| e.image("https://rurigk.com/resources/pot/images/shut.jpg"))
    }).await;
    Ok(())
}

#[poise::command(slash_command)]
pub async fn pato(
    ctx: Context<'_>
) -> Result<(), Error> {
    let _ = ctx.send(|r| {
        r.embed(|e| e.image("https://rurigk.com/resources/pot/images/pato.jpg"))
    }).await;
    Ok(())
}

