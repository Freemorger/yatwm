use std::{collections::HashMap, process::Command};

use log::{error, info};
use x11rb::protocol::xproto::{Keysym, ModMask};

use crate::core::cfgread::ActionEnum;

/// Input contoller struct
#[derive(Debug)]
pub struct InputCt {
    shortcuts: HashMap<Keycut, CutTask>, 
    pub shell: String, // shell command (e.g. sh/bash/zsh)
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

    /// Runs shortcuts (if there one) and returns action enum if 
    /// its needed to be done by YAT State 
    pub fn run_cut(&mut self, cut: Keycut) -> Option<ActionEnum> {
        if let Some(task) = self.shortcuts.get(&cut) {
            match task {
                CutTask::Command(cmd) => {
                    self.run_cmd(&cmd.clone()); 
                }
                CutTask::Action(ac) => {
                    return Some(ac.clone()); 
                }
            }
        };
        None
    }

    pub fn run_cmd(&mut self, cmd: &str) {
        info!("Running command {}", cmd);
        if let Err(e) = Command::new(&self.shell)
            .arg("-c")
            .arg(cmd)
            .spawn() {
            error!("While running {}: {}", cmd, e);
        };
    }
}


#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Keycut {
    ksym: Option<Keysym>,
    modmask: ModMask,
    ranges: KeyRange,
}

impl Keycut {
    pub fn new(sym: Option<Keysym>, modifiers: ModMask, rang: KeyRange) 
        -> Keycut {
        Keycut { 
            ksym: sym, 
            modmask: modifiers,
            ranges: rang
        }
    }
}

#[derive(Debug)]
pub enum CutTask {
    Command(String),
    Action(ActionEnum),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum KeyRange {
    Numbers,
    Any,
    None,
}
