use std::collections::HashMap;

use log::{error, warn};
use maplit::hashmap;
use serde::Deserialize;

#[macro_use]
use maplit;

use crate::core; 

const YATWM_DEF_CFGF: &str = ".config/yatwm/yat.toml"; // prepend home dir

#[derive(Debug, Deserialize)]
pub struct Config {
    pub general: General,
    #[serde(rename = "shortcuts")]
    pub shortcuts: HashMap<String, String>, 
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
            shortcuts: Self::def_shortcuts()
        }
    }

    fn def_shortcuts() -> HashMap<String, String> {
        hashmap! {
            "t".to_string() => "xterm".to_string()
        }    
    }

    fn def_general() -> General {
        General {
            mainmod: "super".to_string()
        }
    }
}  

#[derive(Debug, Deserialize)]
pub struct General {
    mainmod: String, // main modifier key 
}
