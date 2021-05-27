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

    running: Option<Process>,
    pending: VecDeque<process::Command>,

    should_be_removed: bool,

    handler: H,
    command_buffer: channel::Receiver<Command>,
}

struct Process {
    reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    process: process::Child,
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
            running: None,
            should_be_removed: false,
            pending: VecDeque::new(),
            handler,
            command_buffer,
        }
    }

    pub fn init(handler: H, height: usize) -> (Runner<H>, channel::Sender<Command>) {
        let (sender, reciever) = channel::channel(10);
        let runner = Runner::new(handler, height, reciever);
        (runner, sender)
    }

    /// Waits for commands forever
    pub async fn listen(mut self) {
        loop {
            tokio::select! {
                msg = self.command_buffer.recv() => {
                    match msg {
                        Some(Command::Run(cmd)) => self.pending.push_front(cmd),
                        Some(Command::Remove) => self.should_be_removed = true,
                        None => {
                            // oh huh, our only way to communicate with the terminal has been
                            // killed. Probably for the best to just remove everything so we
                            // don't end up with a zombie processes.
                            self.handler.on_terminal_exit(&mut self.window).await;
                            self.clean_command().await;
                            return;
                        },
                    }
                }

                // whenever we're not recieving a signal
                _ = async{} => {
                    match self.running.as_mut() {

                        // we're currently running a command
                        Some(runtime) => {
                            // so lets read another line of stdout
                            if let Some(line) = runtime.reader.next_line().await.unwrap() {
                                self.window += line.clone();
                                self.update_if_should().await;
                            } else {
                                // there are no more lines, must mean the command is finished
                                self.handler.on_command_exit(&mut self.window).await;
                                self.clean_command().await;
                            }
                        },

                        // we're not running a command
                        None => {
                            match self.pending.pop_back() {
                                Some(cmd) => self.run(cmd),
                                None if self.should_be_removed => {
                                    self.handler.on_terminal_exit(&mut self.window).await;
                                    return;
                                }

                                // we have nothing to do. So let's wait a bit to not waste cycles
                                None => tokio::time::sleep(Duration::from_millis(200)).await,
                            }
                        }
                    }
                }
            }
        }
    }

    /// Start execution and monitoring of a shell command
    fn run(&mut self, exec: process::Command) {
        assert!(self.running.is_none());
        let mut child = self.spawn(exec);

        let stdout = child.stdout.take().expect("stdout unavailable");
        let reader = BufReader::new(stdout).lines();

        self.running = Some(Process {
            process: child,
            reader,
        });
    }

    /// Spawn a shell command
    fn spawn(&mut self, mut exec: process::Command) -> process::Child {
        exec.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// checks the timer and updates if needed
    async fn update_if_should(&mut self) {
        let should_update_frame = self.timer.check_and_update(Duration::from_secs(COOLDOWN));
        if should_update_frame {
            self.handler.update(&mut self.window).await;
        }
    }

    /// sets self.running to `None` and makes sure the running process is dead or dies
    async fn clean_command(&mut self) -> Option<Process> {
        let mut cmd = self.running.take()?;

        if cmd.process.id().is_some() {
            // seems to still be running
            cmd.process.kill().await.ok();
        }

        Some(cmd)
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
