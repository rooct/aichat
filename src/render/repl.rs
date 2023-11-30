use super::MarkdownRender;

use crate::repl::{ReplyStreamEvent, SharedAbortSignal};

use anyhow::Result;
use crossbeam::channel::Receiver;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue, style,
    terminal::{self, disable_raw_mode, enable_raw_mode},
};
use std::{
    io::{self, Stdout, Write},
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthStr;

#[allow(clippy::module_name_repetitions)]
pub fn repl_render_stream(
    rx: &Receiver<ReplyStreamEvent>,
    light_theme: bool,
    abort: &SharedAbortSignal,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();

    let ret = repl_render_stream_inner(rx, light_theme, abort, &mut stdout);

    disable_raw_mode()?;

    ret
}

fn repl_render_stream_inner(
    rx: &Receiver<ReplyStreamEvent>,
    light_theme: bool,
    abort: &SharedAbortSignal,
    writer: &mut Stdout,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100);
    let mut buffer = String::new();
    let mut markdown_render = MarkdownRender::new(light_theme);
    let terminal_columns = terminal::size()?.0;
    loop {
        if abort.aborted() {
            return Ok(());
        }

        if let Ok(evt) = rx.try_recv() {
            recover_cursor(writer, terminal_columns, &buffer)?;

            match evt {
                ReplyStreamEvent::Text(text) => {
                    if text.contains('\n') {
                        let text = format!("{buffer}{text}");
                        let mut lines: Vec<&str> = text.split('\n').collect();
                        buffer = lines.pop().unwrap_or_default().to_string();
                        let output = markdown_render.render(&lines.join("\n"));
                        for line in output.split('\n') {
                            queue!(
                                writer,
                                style::Print(line),
                                style::Print("\n"),
                                cursor::MoveLeft(terminal_columns),
                            )?;
                        }
                        queue!(writer, style::Print(&buffer),)?;
                    } else {
                        buffer = format!("{buffer}{text}");
                        let output = markdown_render.render_line_stateless(&buffer);
                        queue!(writer, style::Print(&output))?;
                    }
                    writer.flush()?;
                }
                ReplyStreamEvent::Done => {
                    let output = markdown_render.render_line_stateless(&buffer);
                    let trimed_output = output.trim_end();
                    if !trimed_output.is_empty() {
                        queue!(writer, style::Print(output.trim_end()))?;
                        writer.flush()?;
                    }

                    let (_, row) = cursor::position()?;
                    queue!(writer, cursor::MoveTo(0, row), style::Print("\n\n"))?;
                    writer.flush()?;

                    break;
                }
            }
            continue;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                        abort.set_ctrlc();
                        return Ok(());
                    }
                    KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                        abort.set_ctrld();
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn recover_cursor(writer: &mut Stdout, terminal_columns: u16, buffer: &str) -> Result<()> {
    let buffer_rows = (u16::try_from(buffer.width()).unwrap_or(u16::MAX) + terminal_columns - 1)
        / terminal_columns;
    let (_, row) = cursor::position()?;
    if buffer_rows == 0 {
        queue!(writer, cursor::MoveTo(0, row))?;
    } else if row + 1 >= buffer_rows {
        queue!(writer, cursor::MoveTo(0, row + 1 - buffer_rows))?;
    } else {
        queue!(
            writer,
            terminal::ScrollUp(buffer_rows - 1 - row),
            cursor::MoveTo(0, 0)
        )?;
    }
    Ok(())
}
