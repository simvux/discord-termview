use async_trait::async_trait;
use std::collections::VecDeque;
use std::ops::AddAssign;
use std::process::Stdio;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process;
use tokio::sync::mpsc as channel;

const COOLDOWN: u64 = 4;

/// Create your own listener to capture each frame outputted by the terminal
///
/// Frame rate is low enough to comply with rate limits and will dynamically change depending on
/// the amount output.
#[async_trait]
pub trait Handler {
    async fn update(&mut self, window: &mut Window);
    async fn on_command_exit(&mut self, window: &mut Window);
    async fn on_terminal_exit(&mut self, window: &mut Window);
}

#[derive(Debug)]
pub enum Command {
    Run(process::Command),
    Remove,
}

/// Runner represents the controlled execution of a command where the commands output is being
/// captured into a buffer.
pub struct Runner<H: Handler> {
    window: Window,
    timer: Timer,

    handler: H,
    command_buffer: channel::Receiver<Command>,
}

impl AddAssign<String> for Window {
    fn add_assign(&mut self, line: String) {
        debug_assert!(
            !line.contains('\n'),
            "line characters aren't allowed to be appended to Window"
        );

        self.buffer.push_back(line.into_boxed_str());
        self.shrink_to_limit();
    }
}

impl<H: Handler + Send + 'static> Runner<H> {
    pub fn new(handler: H, height: usize, command_buffer: channel::Receiver<Command>) -> Runner<H> {
        Runner {
            window: Window::new(height),
            timer: Timer {
                // we set it up so that the first update will happen after one second
                last: SystemTime::now() - Duration::from_secs(COOLDOWN + 1),
            },
            handler,
            command_buffer,
        }
    }

    pub fn init(handler: H, height: usize) -> (Runner<H>, channel::Sender<Command>) {
        let (sender, reciever) = channel::channel(10);
        let runner = Runner::new(handler, height, reciever);
        (runner, sender)
    }

    /// Wait for commands forever
    pub async fn listen(mut self) {
        loop {
            match self
                .command_buffer
                .recv()
                .await
                .expect("command_buffer unsafely closed")
            {
                Command::Run(cmd) => {
                    println!("continuing with next queued command");
                    self.run(cmd).await
                }
                Command::Remove => {
                    self.handler.on_terminal_exit(&mut self.window).await;

                    println!("exiting listener for terminal");
                    break;
                }
            }
        }
    }

    /// Block and run a shell command
    async fn run(&mut self, mut exec: process::Command) {
        let mut child = exec
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut reader = BufReader::new(child.stdout.take().unwrap()).lines();

        while let Some(line) = reader.next_line().await.unwrap() {
            self.window += line.clone();

            let should_update_frame = self.timer.check_and_update(Duration::from_secs(COOLDOWN));
            if should_update_frame {
                self.handler.update(&mut self.window).await;
            }
        }

        println!(
            "command exited with status {:?}",
            child.wait().await.unwrap()
        );

        self.handler.on_command_exit(&mut self.window).await;
    }
}

/// Lines of output that adhere to the height limit
pub struct Window {
    pub buffer: VecDeque<Box<str>>,
    pub height: usize,
}

impl Window {
    pub fn new(height: usize) -> Self {
        Window {
            buffer: VecDeque::with_capacity(height),
            height,
        }
    }

    fn over_height_limit(&self) -> bool {
        self.buffer.len() > self.height
    }

    fn shrink_to_limit(&mut self) -> Option<Box<str>> {
        if self.over_height_limit() {
            self.buffer.pop_front()
        } else {
            None
        }
    }
}

struct Timer {
    last: SystemTime,
}

impl Timer {
    fn check_and_update(&mut self, cooldown: Duration) -> bool {
        let now = SystemTime::now();

        let past_limit = now.duration_since(self.last).unwrap() > cooldown;
        if past_limit {
            self.last = now;
        }

        past_limit
    }
}
