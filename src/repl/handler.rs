use crate::client::ChatGptClient;
use crate::config::SharedConfig;
use crate::print_now;
use crate::render::render_stream;

use anyhow::{Context, Result};
use crossbeam::channel::Sender;
use crossbeam::sync::WaitGroup;
use std::cell::RefCell;

use super::abort::SharedAbortSignal;

pub enum ReplCmd {
    Submit(String),
    SetRole(String),
    UpdateConfig(String),
    Prompt(String),
    ClearRole,
    Info,
}

pub struct ReplCmdHandler {
    client: ChatGptClient,
    config: SharedConfig,
    reply: RefCell<String>,
    abort: SharedAbortSignal,
}

impl ReplCmdHandler {
    pub fn init(
        client: ChatGptClient,
        config: SharedConfig,
        abort: SharedAbortSignal,
    ) -> Result<Self> {
        let reply = RefCell::new(String::new());
        Ok(Self {
            client,
            config,
            reply,
            abort,
        })
    }

    pub fn handle(&self, cmd: ReplCmd) -> Result<()> {
        match cmd {
            ReplCmd::Submit(input) => {
                if input.is_empty() {
                    self.reply.borrow_mut().clear();
                    return Ok(());
                }
                let highlight = self.config.lock().highlight;
                let wg = WaitGroup::new();
                let ret = render_stream(
                    &input,
                    &self.client,
                    highlight,
                    true,
                    self.abort.clone(),
                    wg.clone(),
                );
                wg.wait();
                let buffer = ret?;
                self.config.lock().save_message(&input, &buffer)?;
                *self.reply.borrow_mut() = buffer;
            }
            ReplCmd::SetRole(name) => {
                let output = self.config.lock().change_role(&name);
                print_now!("{}\n\n", output.trim_end());
            }
            ReplCmd::ClearRole => {
                self.config.lock().role = None;
                print_now!("\n");
            }
            ReplCmd::Prompt(prompt) => {
                self.config.lock().create_temp_role(&prompt);
                print_now!("\n");
            }
            ReplCmd::Info => {
                let output = self.config.lock().info()?;
                print_now!("{}\n\n", output.trim_end());
            }
            ReplCmd::UpdateConfig(input) => {
                let output = self.config.lock().update(&input)?;
                let output = output.trim();
                if output.is_empty() {
                    print_now!("\n");
                } else {
                    print_now!("{}\n\n", output);
                }
            }
        }
        Ok(())
    }
}

pub struct ReplyStreamHandler {
    sender: Option<Sender<ReplyStreamEvent>>,
    buffer: String,
    abort: SharedAbortSignal,
    repl: bool,
}

impl ReplyStreamHandler {
    pub fn new(
        sender: Option<Sender<ReplyStreamEvent>>,
        repl: bool,
        abort: SharedAbortSignal,
    ) -> Self {
        Self {
            sender,
            abort,
            buffer: String::new(),
            repl,
        }
    }

    pub fn text(&mut self, text: &str) -> Result<()> {
        match self.sender.as_ref() {
            Some(tx) => {
                tx.send(ReplyStreamEvent::Text(text.to_string()))
                    .with_context(|| "Failed to send StreamEvent:Text")?;
            }
            None => {
                print_now!("{}", text);
            }
        }
        self.buffer.push_str(text);
        Ok(())
    }

    pub fn done(&mut self) -> Result<()> {
        match self.sender.as_ref() {
            Some(tx) => {
                tx.send(ReplyStreamEvent::Done)
                    .with_context(|| "Failed to send StreamEvent:Done")?;
            }
            None => {
                if !self.buffer.ends_with('\n') {
                    print_now!("\n")
                }
                if self.repl {
                    print_now!("\n");
                    if cfg!(macos) {
                        print_now!("\n")
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }

    pub fn get_abort(&self) -> SharedAbortSignal {
        self.abort.clone()
    }
}

pub enum ReplyStreamEvent {
    Text(String),
    Done,
}
