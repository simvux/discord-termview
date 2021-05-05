use super::terminal;
use crossbeam::channel;
use terminal::Window;

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
}

impl<ID: std::fmt::Debug + Clone> terminal::Handler for TTYSession<ID> {
    fn update(&mut self, window: &mut Window) {
        let mut snapshot = String::with_capacity(window.buffer.iter().map(|line| line.len()).sum());
        for line in window.buffer.iter().map(Box::as_ref) {
            snapshot.push_str(line);
            snapshot.push('\n');
        }
        snapshot.pop();

        println!("updating terminal `{:?}`", self.id);

        if let Err(e) = self.sender.send((self.id.clone(), Event::Update(snapshot))) {
            eprintln!("TTY {:?} failed to send it's data: {}", self.id, e)
        }
    }

    fn on_command_exit(&mut self, window: &mut Window) {
        self.update(window);

        if let Err(e) = self.sender.send((self.id.clone(), Event::Ready)) {
            eprintln!("TTY {:?} failed to send exit signal: {}", self.id, e)
        }
    }
}
