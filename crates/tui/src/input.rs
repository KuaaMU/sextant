//! Global keybinding handler.

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{ActivePanel, App};

/// Process a single key event and update app state.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.running = false;
        return;
    }

    // Help overlay toggles on top of everything
    if key.code == KeyCode::Char('?') {
        app.show_help = !app.show_help;
        return;
    }

    if app.show_help {
        // Any key closes help
        app.show_help = false;
        return;
    }

    match key.code {
        // Quit
        KeyCode::Char('q') => {
            app.running = false;
        }

        // Panel focus by number
        KeyCode::Char('1') => app.active = ActivePanel::Market,
        KeyCode::Char('2') => app.active = ActivePanel::AgentLog,
        KeyCode::Char('3') => app.active = ActivePanel::Risk,
        KeyCode::Char('4') => app.active = ActivePanel::Orders,
        KeyCode::Char('5') => app.active = ActivePanel::Research,
        KeyCode::Char('6') => app.active = ActivePanel::Memory,

        // Tab cycling
        KeyCode::Tab => app.active = app.active.next(),
        KeyCode::BackTab => app.active = app.active.prev(),

        // Pause/resume
        KeyCode::Char(' ') => {
            app.paused = !app.paused;
        }

        // Zen mode
        KeyCode::Char('z') => {
            app.zen_mode = !app.zen_mode;
        }

        _ => {}
    }
}

/// Poll for a key event (non-blocking).
pub fn poll_key() -> Option<KeyEvent> {
    if event::poll(std::time::Duration::from_millis(0)).ok()? {
        if let Event::Key(key) = event::read().ok()? {
            return Some(key);
        }
    }
    None
}
