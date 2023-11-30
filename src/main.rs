mod cli;
mod client;
mod config;
mod render;
mod repl;
mod term;
#[macro_use]
mod utils;

use crate::cli::Cli;
use crate::client::ChatGptClient;
use crate::config::{Config, SharedConfig};

use anyhow::{anyhow, Result};
use clap::Parser;
use crossbeam::sync::WaitGroup;
use is_terminal::IsTerminal;
use parking_lot::Mutex;
use render::{render_stream, MarkdownRender};
use repl::{AbortSignal, Repl};
use std::io::{stdin, Read};
use std::sync::Arc;
use std::{io::stdout, process::exit};
use utils::cl100k_base_singleton;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let text = cli.text();
    let config = Arc::new(Mutex::new(Config::init(text.is_none())?));
    if cli.list_roles {
        config
            .lock()
            .roles
            .iter()
            .for_each(|v| println!("{}", v.name));
        exit(0);
    }
    let role = match &cli.role {
        Some(name) => Some(
            config
                .lock()
                .find_role(name)
                .ok_or_else(|| anyhow!("Unknown role '{name}'"))?,
        ),
        None => None,
    };
    config.lock().role = role;
    if cli.no_highlight {
        config.lock().highlight = false;
    }
    let no_stream = cli.no_stream;
    let client = ChatGptClient::init(config.clone())?;
    if atty::isnt(atty::Stream::Stdin) {
        let mut input = String::new();
        stdin().read_to_string(&mut input)?;
        if let Some(text) = text {
            input = format!("{text}\n{input}");
        }
        start_directive(client, config, &input, no_stream)
    } else {
        match text {
            Some(text) => start_directive(client, config, &text, no_stream),
            None => start_interactive(client, config),
        }
    }
}

fn start_directive(
    client: ChatGptClient,
    config: SharedConfig,
    input: &str,
    no_stream: bool,
) -> Result<()> {
    let highlight = config.lock().highlight && stdout().is_terminal();
    let output = if no_stream {
        let output = client.send_message(input)?;
        if highlight {
            let mut markdown_render = MarkdownRender::new();
            println!("{}", markdown_render.render(&output).trim());
        } else {
            println!("{}", output.trim());
        }
        output
    } else {
        let wg = WaitGroup::new();
        let abort = AbortSignal::new();
        let abort_clone = abort.clone();
        ctrlc::set_handler(move || {
            abort_clone.set_ctrlc();
        })
        .expect("Error setting Ctrl-C handler");
        let output = render_stream(input, &client, highlight, false, abort, wg.clone())?;
        wg.wait();
        output
    };
    config.lock().save_message(input, &output)
}

fn start_interactive(client: ChatGptClient, config: SharedConfig) -> Result<()> {
    cl100k_base_singleton();
    config.lock().on_repl()?;
    let mut repl = Repl::init(config.clone())?;
    repl.run(client, config)
}
