use std::collections::HashMap;

use log::{error, warn};
use maplit::hashmap;
use serde::Deserialize;

#[macro_use]
use maplit;
use x11rb::{connection::Connection, protocol::xproto::{ConnectionExt, Keycode}};
use xkb::Keysym;

use crate::core::{self, CustomError}; 

const YATWM_DEF_CFGF: &str = ".config/yatwm/yat.toml"; // prepend home dir

#[derive(Debug, Deserialize)]
pub struct Config {
    pub general: General,
    pub shortcuts: HashMap<String, ActionEnum>,
    pub macros: Option<HashMap<String, CfgMacro>>,
} 

impl Config {
    pub fn from_def_dir() -> Config {
        let path = core::get_homedpath(YATWM_DEF_CFGF, false).unwrap();

        match std::fs::read_to_string(path) {
            Ok(fc) => {
                let res = toml::from_str(&fc);
                match res {
                    Ok(v) => {return v;}
                    Err(e) => {
                        error!("Error reading config: {}", e);
                        Self::def()
                    }
                }
            }
            Err(e) => {
                warn!("Can't open config file: {}", e);
                Self::def()
            }
        }
    }

    fn def() -> Config {
        Config {
            general: Self::def_general(),
            shortcuts: Self::def_shortcuts(),
            macros: None,
        }
    }

    fn def_shortcuts() -> HashMap<String, ActionEnum> {
        hashmap! {
            "t".to_string() => ActionEnum::Command("xterm".to_string()),
            "super+alt+r".to_string() => ActionEnum::CfgReload(0),
        }    
    }

    fn def_general() -> General {
        General {
            mainmod: "super".to_string(),
            sh: None,
            focus_new: Some(true),
            def_wrksp_ctr: None,
            autostart: None,
        }
    }
}  

pub fn keysym_to_keycode<C: Connection>(conn: &C, target_sym: Keysym)
    -> Option<Keycode> {
    // TODO: optimize it, like caching or so
    let setup = conn.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;

    let map = conn.get_keyboard_mapping(min, max - min - 1)
        .ok()?.reply().ok()?;

    for (i, syms) in map.keysyms.chunks(map.keysyms_per_keycode as usize)
        .enumerate() {
        if syms.iter().any(|&s| s == target_sym.0) {
            return Some(min + i as u8)
        }
    }

    None
}

pub fn keycode_to_keysym<C: Connection>(conn: &C, code: u8) 
    -> Option<Keysym> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;

    let map = conn.get_keyboard_mapping(min, max - min - 1)
        .ok()?.reply().ok()?;

    let idx = (code - min) as usize * map.keysyms_per_keycode as usize;
    let keysym = map.keysyms[idx];
    Some(xkb::Keysym(keysym))
}

#[derive(Debug, Deserialize)]
pub struct General {
    pub mainmod: String, // main modifier key
    pub sh: Option<String>,
    pub focus_new: Option<bool>,
    pub def_wrksp_ctr: Option<usize>,
    pub autostart: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ActionEnum {
    Command(String),
    SwitchWorkspace(usize), 
    DeltaWorkspace(isize),
    MoveToWorkspace(usize),
    Complex(Vec<ActionEnum>),
    CfgReload(usize),
    FocusOther(isize),
    ExpandMacro(String, ActionValue), // macro name
}

impl ActionEnum {
    pub fn replace_val(&self, new_val: &ActionValue) 
        -> Result<ActionEnum, Box<dyn std::error::Error>> {
        match (self.clone(), new_val.clone()) {
            (ActionEnum::Command(_), ActionValue::Str(s)) => {
                Ok(ActionEnum::Command(s.clone()))
            }
            (ActionEnum::FocusOther(_), ActionValue::Int(i)) => {
                Ok(ActionEnum::FocusOther(i))
            }
            (ActionEnum::MoveToWorkspace(_), ActionValue::Uint(u)) => {
                Ok(ActionEnum::MoveToWorkspace(u))
            }
            (ActionEnum::DeltaWorkspace(_), ActionValue::Int(i)) => {
                Ok(ActionEnum::DeltaWorkspace(i))
            }
            (ActionEnum::SwitchWorkspace(_), ActionValue::Uint(u)) => {
                Ok(ActionEnum::SwitchWorkspace(u))
            }
            other => Err(Box::new(CustomError {
                message: format!("{:?} couldn't be replaced with {:?}",
                             other.0, other.1)
            }))
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CfgMacro {
    Define(String), // like in C
    DefineActions(Vec<ActionEnum>),
    ReplaceAll(Vec<ActionEnum>, ActionValue),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ActionValue {
    Uint(usize),
    Int(isize),
    Str(String),
    ActVec(Vec<ActionEnum>),
    GetFromCut, // gets key
    None,
}
