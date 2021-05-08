use super::terminal;
use async_trait::async_trait;
use std::collections::VecDeque;
use terminal::Window;
use tokio::sync::mpsc as channel;

pub enum Event {
    Update(String),
    Ready,
}

/// Proxy between a Runner and a combinator
pub struct TTYSession<ID> {
    id: ID,
    sender: channel::Sender<(ID, Event)>,
}

impl<ID> TTYSession<ID> {
    pub fn new(id: ID, sender: channel::Sender<(ID, Event)>) -> Self {
        Self { id, sender }
    }

    pub fn append_prompt(&self, window: &mut Window) {
        let prompt = String::from(" >>> ");
        window.buffer.push_back(prompt.into_boxed_str());
    }
}

fn render_snapshot(buffer: &VecDeque<Box<str>>) -> String {
    let mut snapshot = String::with_capacity(buffer.iter().map(|line| line.len()).sum());
    for line in buffer.iter().map(Box::as_ref) {
        snapshot.push_str(line);
        snapshot.push('\n');
    }
    snapshot.pop();
    snapshot
}

#[async_trait]
impl<ID: std::fmt::Debug + Clone + Send + Sync> terminal::Handler for TTYSession<ID> {
    async fn update(&mut self, window: &mut Window) {
        println!("updating terminal `{:?}`", self.id);

        let snapshot = render_snapshot(&window.buffer);

        if let Err(e) = self
            .sender
            .send((self.id.clone(), Event::Update(snapshot)))
            .await
        {
            eprintln!("TTY {:?} failed to send it's data: {}", self.id, e)
        }
    }

    async fn on_command_exit(&mut self, window: &mut Window) {
        self.append_prompt(window);

        self.update(window).await;

        if let Err(e) = self.sender.send((self.id.clone(), Event::Ready)).await {
            eprintln!("TTY {:?} failed to send exit signal: {}", self.id, e)
        }
    }

    async fn on_terminal_exit(&mut self, window: &mut Window) {
        window
            .buffer
            .push_back(String::from(" <session closed> ").into_boxed_str());

        self.update(window).await
    }
}
