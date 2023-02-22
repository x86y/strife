use _tui::backend;
use crossterm::{event, terminal};
use std::{io, mem};

pub use _tui::{layout, style, text, widgets, Frame};

pub type Backend = backend::CrosstermBackend<io::Stdout>;

/// Interface to the terminal.
pub struct Terminal {
    damaged: bool,
    terminal: _tui::Terminal<Backend>,
}

impl Terminal {
    /// Wrapper around terminal initialisation.
    pub fn new() -> io::Result<Self> {
        let backend = Backend::new(io::stdout());
        let terminal = _tui::Terminal::new(backend)?;
        let mut terminal = Self {
            damaged: true,
            terminal,
        };

        terminal.enable_app_mode()?;

        Ok(terminal)
    }

    /// Enable application mode.
    ///
    /// Enables raw terminal processing, bracketed pasting,
    /// focus detction, and mouse event processing.
    pub fn enable_app_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;

        crossterm::execute!(
            self.terminal.backend_mut(),
            terminal::EnterAlternateScreen,
            event::EnableBracketedPaste,
            event::EnableFocusChange,
            event::EnableMouseCapture,
        )
    }

    /// Disable application mode.
    ///
    /// Reverts all things enabled by [`enable_app_mode`](Self::enable_app_mode).
    pub fn disable_app_mode(&mut self) -> io::Result<()> {
        crossterm::execute!(
            self.terminal.backend_mut(),
            event::DisableBracketedPaste,
            event::DisableFocusChange,
            event::DisableMouseCapture,
            terminal::LeaveAlternateScreen,
        )?;

        terminal::disable_raw_mode()
    }

    /// Mark the terminal as damaged.
    pub fn damage(&mut self) {
        self.damaged = true;
    }

    /// Clears the damage tracker, synchronizes terminal size,
    /// calls the render function, and flushes the internal state.
    pub fn render<R>(&mut self, render: R) -> io::Result<()>
    where
        R: FnOnce(&mut Frame<'_, Backend>),
    {
        if !mem::take(&mut self.damaged) {
            return Ok(());
        }

        self.terminal.draw(render)?;

        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.disable_app_mode();
    }
}
