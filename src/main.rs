use serenity::prelude::*;
use std::io::Write;

pub mod discord;
pub mod parser;
pub mod session;
pub mod terminal;

struct FileWriter {
    f: std::fs::File,
}

impl terminal::Handler for FileWriter {
    fn update(&mut self, window: &mut terminal::Window) {
        self.f.set_len(0).expect("failed to clear file");
        println!("{:#?}", &window.buffer);
        for line in window.buffer.iter() {
            write!(self.f, "{}", line).unwrap();
        }
    }

    fn on_command_exit(&mut self, window: &mut terminal::Window) {
        self.update(window)
    }
}

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
