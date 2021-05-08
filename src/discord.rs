use super::{parser, session, terminal};
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId, id::MessageId, id::RoleId},
    prelude::*,
};
use std::collections::HashMap;
use tokio::process;
use tokio::sync::mpsc as channel;
use tokio::sync::Mutex;

pub type Packet = ((ChannelId, MessageId), session::Event);
type TermID = String;

const FRAME_BUFFERING: usize = 5;
const DISCORD_LENGTH_LIMIT: usize = 2000;

pub struct Handler {
    frame_sender: channel::Sender<Packet>,
    frame_reciever: Mutex<Option<channel::Receiver<Packet>>>,

    settings: Settings,
    ttys: Mutex<HashMap<TermID, channel::Sender<terminal::Command>>>,
}

pub struct Settings {
    allowed_roles: Vec<RoleId>,
    prefix: u8,
}

impl Settings {
    pub fn new(allowed_roles: Vec<serenity::model::id::RoleId>, seperator: u8) -> Self {
        Self {
            allowed_roles,
            prefix: seperator,
        }
    }

    pub fn parse() -> Self {
        let seperator = std::env::var("SEPERATOR")
            .map(|s| s.as_bytes()[0])
            .unwrap_or(b'$');

        let allowed_roles = std::env::var("ALLOWED_ROLES")
            .expect("missing semi-colon ALLOWED_ROLES variable containing channel ID's")
            .split(';')
            .map(|word| word.parse().map(RoleId))
            .collect::<Result<Vec<RoleId>, _>>()
            .expect("ALLOWED_ROLES is expected to be a semi-colon seperated list of role ID's in numeric format");

        Settings {
            allowed_roles,
            prefix: seperator,
        }
    }
}

enum Error {
    Parser(parser::Error),
    NoTerminal(TermID),
    CannotRespond,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Parser(err) => err.fmt(f),
            Error::NoTerminal(term) => write!(f, "terminal `{}` not found", term),
            Error::CannotRespond => f.write_str("cannot respond to message. Missing permissions?"),
        }
    }
}

impl Handler {
    pub fn new(settings: Settings) -> Self {
        let (frame_sender, frame_reciever) = channel::channel(FRAME_BUFFERING);

        Self {
            frame_sender,
            frame_reciever: Mutex::new(Some(frame_reciever)),
            settings,
            ttys: Mutex::new(HashMap::new()),
        }
    }

    async fn is_authorized(&self, _ctx: &Context, msg: &Message) -> bool {
        for role in &self.settings.allowed_roles {
            if msg.member.as_ref().unwrap().roles.contains(role) {
                return true;
            }
        }

        false
    }

    async fn parse_and_apply_command(
        &self,
        ctx: &Context,
        msg: &Message,
        term: TermID,
        cmd: &str,
    ) -> Result<(), Error> {
        let action = parser::parse(cmd).map_err(Error::Parser)?;
        dbg!(&action);

        match action {
            parser::Command::New { height, private } => {
                self.apply_new(ctx, msg, term, height, private).await
            }
            parser::Command::Remove => self.apply_remove(ctx, msg, term).await,
            parser::Command::Run(cmd) => self.apply_run(term, cmd).await,
        }
    }

    async fn apply_new(
        &self,
        ctx: &Context,
        msg: &Message,
        term: TermID,
        height: usize,
        private: bool,
    ) -> Result<(), Error> {
        let tty = self.ttys.lock().await.get(&term).cloned();
        match tty {
            Some(sender) => {
                // send exit signal; then create new
                sender.send(terminal::Command::Exit).await.unwrap();

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                self.spawn_new_terminal(ctx, msg, term, height, private)
                    .await
            }
            None => {
                self.spawn_new_terminal(ctx, msg, term, height, private)
                    .await
            }
        }
    }

    async fn apply_remove(
        &self,
        _ctx: &Context,
        _msg: &Message,
        term: TermID,
    ) -> Result<(), Error> {
        let tty = self.ttys.lock().await.get(&term).cloned();
        tty.ok_or_else(|| Error::NoTerminal(term.clone()))?
            .send(terminal::Command::Exit)
            .await
            .ok();

        self.ttys.lock().await.remove(&term);

        Ok(())
    }

    async fn spawn_new_terminal(
        &self,
        ctx: &Context,
        msg: &Message,
        term: TermID,
        height: usize,
        _private: bool,
    ) -> Result<(), Error> {
        let reply = msg
            .reply(ctx, render_terminal_layout(" >>> "))
            .await
            .map_err(|_| Error::CannotRespond)?;

        let ttysession =
            session::TTYSession::new((msg.channel_id, reply.id), self.frame_sender.clone());

        let (runner, command_sender) = terminal::Runner::init(ttysession, height);

        if let Some(_existing) = self.ttys.lock().await.insert(term.clone(), command_sender) {
            eprintln!(
                "WARNING: tty `{}` refused to die in time, this might create a zombie process",
                term
            )
        }

        tokio::spawn(async move { runner.listen().await });

        Ok(())
    }

    async fn apply_run(&self, term: TermID, mut cmd: String) -> Result<(), Error> {
        println!("applying `{}` onto {}", cmd, term);

        let sender = self
            .ttys
            .lock()
            .await
            .get(&term)
            .cloned()
            .ok_or(Error::NoTerminal(term))?;

        // TODO: Fix this
        // temporary hack to include stderr in discord terminals
        cmd.push_str(" 2>&1");

        let mut shell = process::Command::new("bash");
        shell.arg("-c").arg(&cmd);

        println!("handing the command to the terminal instance");
        sender.send(terminal::Command::Run(shell)).await.unwrap();

        Ok(())
    }

    async fn respond_with_error(&self, ctx: &Context, error: Error, channel: ChannelId) {
        eprintln!("user error: {}", error);

        if let Err(e) = channel
            .send_message(ctx, |m| {
                m.content(format!("error: {}", error));
                m
            })
            .await
        {
            eprintln!("failed to present error in channel: {}", e)
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content.as_bytes().first() == Some(&self.settings.prefix)
            && self.is_authorized(&ctx, &msg).await
        {
            println!("parsing {}", &msg.content);

            let tty_identifier = {
                let pos = msg.content.as_bytes()[1..]
                    .iter()
                    .position(|&b| b == b' ')
                    .unwrap_or(msg.content.len() - 1);

                msg.content[1..=pos].to_string()
            };

            let cmd_portion = msg.content[tty_identifier.len() + 2..].trim();

            if let Err(e) = self
                .parse_and_apply_command(&ctx, &msg, tty_identifier, cmd_portion)
                .await
            {
                self.respond_with_error(&ctx, e, msg.channel_id).await;
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("connected to discord as {}", ready.user.name);

        let mut renderer = Renderer {
            frame_reciever: self
                .frame_reciever
                .lock()
                .await
                .take()
                .expect("no reciever channel"),
        };

        tokio::spawn(async move { renderer.render_pipeline(ctx).await });
    }
}

fn render_terminal_layout<C: std::fmt::Display>(contents: C) -> String {
    format!("```\n{}```", contents)
}

struct Renderer {
    frame_reciever: channel::Receiver<Packet>,
}

impl Renderer {
    async fn render_pipeline(&mut self, ctx: Context) {
        loop {
            let ((channelid, messageid), event) = self.frame_reciever.recv().await.unwrap();

            match event {
                session::Event::Ready => {
                    println!("terminal {} finished it's command", messageid);
                }
                session::Event::Update(frame) => {
                    if let Err(e) = self.refresh(&ctx, channelid, messageid, frame).await {
                        eprintln!("frame update error: {}", e);
                    };
                }
            }
        }
    }

    async fn refresh(
        &self,
        ctx: &Context,
        channelid: ChannelId,
        messageid: MessageId,
        mut frame: String,
    ) -> Result<Message, serenity::Error> {
        while frame.len() > (DISCORD_LENGTH_LIMIT - 10) {
            // `- 10` because formatting hasn't been applied

            println!(
                "shrinking message since discord message length limit is exceded even with height limitation"
            );

            let line_end = frame.find(|c| c == '\n').unwrap();
            frame.replace_range(0..=line_end, "");
        }

        channelid
            .edit_message(&ctx, messageid, |m| {
                m.content(render_terminal_layout(frame));
                m
            })
            .await
    }
}
