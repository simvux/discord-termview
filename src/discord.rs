use super::{parser, session, terminal};
use crossbeam::channel;
use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId, id::MessageId, id::RoleId},
    prelude::*,
};
use std::collections::HashMap;
use std::process;

pub type Packet = ((ChannelId, MessageId), session::Event);
type TermID = u8;

const FRAME_BUFFERING: usize = 10;

pub struct Handler {
    frame_sender: channel::Sender<Packet>,
    frame_reciever: channel::Receiver<Packet>,

    settings: Settings,
    ttys: Mutex<HashMap<TermID, channel::Sender<terminal::Command>>>,
}

pub struct Settings {
    allowed_roles: Vec<RoleId>,
    seperator: u8,
}

impl Settings {
    pub fn new(allowed_roles: Vec<serenity::model::id::RoleId>, seperator: u8) -> Self {
        Self {
            allowed_roles,
            seperator,
        }
    }

    pub fn parse() -> Self {
        let seperator = std::env::var("SEPERATOR")
            .map(|s| s.as_bytes()[0])
            .unwrap_or(b':');

        let allowed_roles = std::env::var("ALLOWED_ROLES")
            .expect("missing semi-colon ALLOWED_ROLES variable containing channel ID's")
            .split(';')
            .map(|word| word.parse().map(RoleId))
            .collect::<Result<Vec<RoleId>, _>>()
            .expect("ALLOWED_ROLES is expected to be a semi-colon seperated list of role ID's in numeric format");

        Settings {
            allowed_roles,
            seperator,
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
            Error::NoTerminal(term) => write!(f, "terminal `{}` not found", *term as char),
            Error::CannotRespond => f.write_str("cannot respond to message. Missing permissions?"),
        }
    }
}

impl Handler {
    pub fn new(settings: Settings) -> Self {
        let (frame_sender, frame_reciever) = crossbeam::channel::bounded(FRAME_BUFFERING);

        Self {
            frame_sender,
            frame_reciever,
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
        let ttys = self.ttys.lock().await;
        match ttys.get(&term) {
            Some(sender) => {
                // send exit signal; then create new
                sender.send(terminal::Command::Exit).unwrap();
                drop(ttys);

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                self.spawn_new_terminal(ctx, msg, term, height, private)
                    .await
            }
            None => {
                drop(ttys);
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
        let mut ttys = self.ttys.lock().await;
        ttys.get(&term)
            .ok_or(Error::NoTerminal(term))?
            .send(terminal::Command::Exit)
            .ok();
        ttys.remove(&term);
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
            .reply(ctx, render_terminal_layout(String::new()))
            .await
            .map_err(|_| Error::CannotRespond)?;

        let ttysession =
            session::TTYSession::new((msg.channel_id, reply.id), self.frame_sender.clone());

        let (runner, command_sender) = terminal::Runner::init(ttysession, height);

        if let Some(_existing) = self.ttys.lock().await.insert(term, command_sender) {
            eprintln!(
                "WARNING: tty `{}` refused to die in time, this might create a zombie process",
                term
            )
        }

        std::thread::spawn(|| runner.listen());

        Ok(())
    }

    async fn apply_run(&self, term: TermID, cmd: String) -> Result<(), Error> {
        println!("applying `{}` onto {}", cmd, term as char);

        let ttys = self.ttys.lock().await;
        let sender = ttys.get(&term).ok_or(Error::NoTerminal(term))?;

        let mut shell = process::Command::new("bash");
        shell.arg("-c").arg(&cmd);

        println!("handing the command to the terminal instance");
        sender.send(terminal::Command::Run(shell)).unwrap();

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
        if msg.content.bytes().nth(1) == Some(self.settings.seperator)
            && self.is_authorized(&ctx, &msg).await
        {
            println!("{}", &msg.content);

            let tty_identifier = msg.content.as_bytes()[0];

            if let Err(e) = self
                .parse_and_apply_command(&ctx, &msg, tty_identifier, msg.content[2..].trim())
                .await
            {
                self.respond_with_error(&ctx, e, msg.channel_id).await;
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("connected to discord as {}", ready.user.name);

        let renderer = Renderer {
            frame_reciever: self.frame_reciever.clone(),
        };
        tokio::spawn(async move { renderer.render_pipeline(ctx).await });
    }
}

fn render_terminal_layout(contents: String) -> String {
    format!("```\n{}```", contents)
}

struct Renderer {
    frame_reciever: channel::Receiver<Packet>,
}

impl Renderer {
    async fn render_pipeline(&self, ctx: Context) {
        loop {
            let ((channelid, messageid), event) = self.frame_reciever.recv().unwrap();

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
        frame: String,
    ) -> Result<Message, serenity::Error> {
        channelid
            .edit_message(&ctx, messageid, |m| {
                m.content(render_terminal_layout(frame));
                m
            })
            .await
    }
}
