mod cmd;
mod markdown;
mod repl;

use self::cmd::cmd_render_stream;
pub use self::markdown::MarkdownRender;
use self::repl::repl_render_stream;

use crate::client::ChatGptClient;
use crate::print_now;
use crate::repl::{ReplyStreamHandler, SharedAbortSignal};
use anyhow::Result;
use crossbeam::channel::unbounded;
use crossbeam::sync::WaitGroup;
use std::thread::spawn;

pub fn render_stream(
    input: &str,
    prompt: Option<String>,
    client: &ChatGptClient,
    highlight: bool,
    repl: bool,
    abort: SharedAbortSignal,
    wg: WaitGroup,
) -> Result<String> {
    let mut stream_handler = if highlight {
        let (tx, rx) = unbounded();
        let abort_clone = abort.clone();
        spawn(move || {
            let err = if repl {
                repl_render_stream(rx, abort)
            } else {
                cmd_render_stream(rx, abort)
            };
            if let Err(err) = err {
                let err = format!("{err:?}");
                print_now!("{}\n\n", err.trim());
            }
            drop(wg);
        });
        ReplyStreamHandler::new(Some(tx), abort_clone)
    } else {
        drop(wg);
        ReplyStreamHandler::new(None, abort)
    };
    client.send_message_streaming(input, prompt, &mut stream_handler)?;
    let buffer = stream_handler.get_buffer();
    Ok(buffer.to_string())
}
