use serenity::prelude::*;

pub mod discord;
pub mod parser;
pub mod session;
pub mod terminal;

#[tokio::main]
async fn main() {
    let token =
        std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN does not contain a valid token");

    let settings = discord::Settings::parse();

    let mut client = Client::builder(&token)
        .event_handler(discord::Handler::new(settings))
        .await
        .expect("error creating client");

    if let Err(e) = client.start().await {
        eprintln!("Client error: {:?}", e);
    }
}
