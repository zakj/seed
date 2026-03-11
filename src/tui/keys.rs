use ratatui::crossterm::event::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Quit,
    TogglePanel,
    NavigateDown,
    NavigateUp,
    Collapse,
    Expand,
    Toggle,
    First,
    Last,
    EditTitle,
    EditDescription,
    AddTask,
    AddChildTask,
    StartTask,
    CompleteTask,
    DropTask,
    PriorityMode,
    ScrollDown,
    ScrollUp,
    // Priority sub-mode
    SetCritical,
    SetHigh,
    SetNormal,
    SetLow,
    Cancel,
}

pub struct Hint {
    pub keys: &'static [(KeyCode, Command)],
    pub label: &'static str,
    pub description: &'static str,
}

pub const GLOBAL: &[Hint] = &[
    Hint {
        keys: &[(KeyCode::Char('q'), Command::Quit)],
        label: "q",
        description: "quit",
    },
    Hint {
        keys: &[(KeyCode::Tab, Command::TogglePanel)],
        label: "Tab",
        description: "switch pane",
    },
];

pub const TREE: &[Hint] = &[
    Hint {
        keys: &[
            (KeyCode::Char('j'), Command::NavigateDown),
            (KeyCode::Down, Command::NavigateDown),
        ],
        label: "j/k",
        description: "navigate",
    },
    Hint {
        keys: &[
            (KeyCode::Char('k'), Command::NavigateUp),
            (KeyCode::Up, Command::NavigateUp),
        ],
        label: "",
        description: "",
    },
    Hint {
        keys: &[
            (KeyCode::Char('h'), Command::Collapse),
            (KeyCode::Left, Command::Collapse),
        ],
        label: "h/l",
        description: "collapse/expand",
    },
    Hint {
        keys: &[
            (KeyCode::Char('l'), Command::Expand),
            (KeyCode::Right, Command::Expand),
        ],
        label: "",
        description: "",
    },
    Hint {
        keys: &[(KeyCode::Char(' '), Command::Toggle)],
        label: "Space",
        description: "toggle",
    },
    Hint {
        keys: &[(KeyCode::Char('e'), Command::EditTitle)],
        label: "e",
        description: "edit",
    },
    Hint {
        keys: &[(KeyCode::Char('E'), Command::EditDescription)],
        label: "E",
        description: "describe",
    },
    Hint {
        keys: &[(KeyCode::Char('a'), Command::AddTask)],
        label: "a/A",
        description: "add/add child",
    },
    Hint {
        keys: &[(KeyCode::Char('A'), Command::AddChildTask)],
        label: "",
        description: "",
    },
    Hint {
        keys: &[(KeyCode::Char('s'), Command::StartTask)],
        label: "s/d/x",
        description: "start/done/drop",
    },
    Hint {
        keys: &[(KeyCode::Char('d'), Command::CompleteTask)],
        label: "",
        description: "",
    },
    Hint {
        keys: &[(KeyCode::Char('x'), Command::DropTask)],
        label: "",
        description: "",
    },
    Hint {
        keys: &[(KeyCode::Char('p'), Command::PriorityMode)],
        label: "p",
        description: "priority",
    },
    Hint {
        keys: &[
            (KeyCode::Char('g'), Command::First),
            (KeyCode::Home, Command::First),
        ],
        label: "g/G",
        description: "top/bottom",
    },
    Hint {
        keys: &[
            (KeyCode::Char('G'), Command::Last),
            (KeyCode::End, Command::Last),
        ],
        label: "",
        description: "",
    },
];

pub const DETAIL: &[Hint] = &[
    Hint {
        keys: &[
            (KeyCode::Char('j'), Command::ScrollDown),
            (KeyCode::Down, Command::ScrollDown),
        ],
        label: "j/k",
        description: "scroll",
    },
    Hint {
        keys: &[
            (KeyCode::Char('k'), Command::ScrollUp),
            (KeyCode::Up, Command::ScrollUp),
        ],
        label: "",
        description: "",
    },
    Hint {
        keys: &[
            (KeyCode::Char('g'), Command::First),
            (KeyCode::Home, Command::First),
        ],
        label: "g/G",
        description: "top/bottom",
    },
    Hint {
        keys: &[
            (KeyCode::Char('G'), Command::Last),
            (KeyCode::End, Command::Last),
        ],
        label: "",
        description: "",
    },
];

pub const PRIORITY: &[Hint] = &[
    Hint {
        keys: &[(KeyCode::Char('c'), Command::SetCritical)],
        label: "c",
        description: "critical",
    },
    Hint {
        keys: &[(KeyCode::Char('h'), Command::SetHigh)],
        label: "h",
        description: "high",
    },
    Hint {
        keys: &[(KeyCode::Char('n'), Command::SetNormal)],
        label: "n",
        description: "normal",
    },
    Hint {
        keys: &[(KeyCode::Char('l'), Command::SetLow)],
        label: "l",
        description: "low",
    },
    Hint {
        keys: &[(KeyCode::Esc, Command::Cancel)],
        label: "Esc",
        description: "cancel",
    },
];

pub fn resolve(tables: &[&[Hint]], code: KeyCode) -> Option<Command> {
    tables
        .iter()
        .flat_map(|t| t.iter())
        .flat_map(|h| h.keys.iter())
        .find(|(k, _)| *k == code)
        .map(|(_, cmd)| *cmd)
}
