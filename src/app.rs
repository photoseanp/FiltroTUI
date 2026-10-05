use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    ProgramSettings,
    CareCenter,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Overview, Tab::ProgramSettings, Tab::CareCenter];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::ProgramSettings => "Program Settings",
            Tab::CareCenter => "Care Center",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    pub fn next(self) -> Tab {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Tab {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

pub struct App {
    pub tab: Tab,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Overview,
            should_quit: false,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Tab => {
                self.tab = self.tab.next();
                return;
            }
            KeyCode::BackTab => {
                self.tab = self.tab.prev();
                return;
            }
            _ => {}
        }

        match self.tab {
            Tab::Overview => {}
            Tab::ProgramSettings => {}
            Tab::CareCenter => {}
        }
    }

    pub fn on_tick(&mut self) {
        // RS485 polling will go here.
    }
}
