use std::{collections::HashMap, process::Command};

use log::{error, info};
use x11rb::protocol::xproto::{Keysym, ModMask};

/// Input contoller struct
#[derive(Debug)]
pub struct InputCt {
    shortcuts: HashMap<Keycut, CutTask>, 
    shell: String, // shell command (e.g. sh/bash/zsh)
}

impl InputCt {
    pub fn new(shell: Option<String>) -> InputCt {
        InputCt { 
            shortcuts: HashMap::new(),
            shell: shell.unwrap_or("sh".to_owned())
        }
    }

    pub fn add_shortcut(&mut self, cut: Keycut, task: CutTask) {
        self.shortcuts.insert(cut, task);
    }

    pub fn run_cut(&mut self, cut: Keycut) {
        if let Some(task) = self.shortcuts.get(&cut) {
            match task {
                CutTask::Command(cmd) => {
                    info!("Running command {}", cmd);
                    if let Err(e) = Command::new(&self.shell)
                        .arg("-c")
                        .arg(cmd)
                        .spawn() {
                        error!("While running {}: {}", cmd, e);
                    };
                    }
                }
        };
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Keycut {
    ksym: Keysym,
    modmask: ModMask,
}

impl Keycut {
    pub fn new(sym: Keysym, modifiers: ModMask) -> Keycut {
        Keycut { 
            ksym: sym, 
            modmask: modifiers,
        }
    }
}

#[derive(Debug)]
pub enum CutTask {
    Command(String),
}
