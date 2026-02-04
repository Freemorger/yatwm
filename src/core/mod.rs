use std::{collections::HashMap, path::PathBuf, str::FromStr};

use indexmap::IndexMap;
use log::{error, info, warn};
use x11rb::{COPY_DEPTH_FROM_PARENT, connection::Connection, protocol::{Event, xproto::{ButtonIndex, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, GrabMode, InputFocus, ModMask, Screen, WindowClass}}, rust_connection::RustConnection};

use crate::core::{
    cfgread::{ActionEnum, Config, keycode_to_keysym, keysym_to_keycode}, input::{InputCt, Keycut}
};

pub mod cfgread;
pub mod input;

const YATWM_DEF_LOGF: &str = ".local/state/yatwm.log"; // in homedir. prepend home 

pub struct WM {
    cfg: Config,
    state: YATState<RustConnection>
}

impl WM {
    pub fn new() -> WM {
        // configuring fern for logging
        let log_path = get_homedpath(YATWM_DEF_LOGF, true).unwrap();

        Self::prepare_log(&log_path);

        let cfg = Config::from_def_dir();

        log::info!("Connecting to X server..");
        let (conn, scr_num) = x11rb::connect(None).unwrap();
        log::info!("Connected successful");

        let mut state = YATState::new(conn, scr_num, &cfg);
        
        state.reg_scuts(&cfg);

        WM {
            cfg: cfg,
            state: state
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.state.conn.change_window_attributes(
            self.state.screen.root,
            &ChangeWindowAttributesAux::new().event_mask(
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY
                | EventMask::ENTER_WINDOW
            ),
        )?;

        self.state.conn.grab_button(
            true,               
            self.state.screen.root,      
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
            GrabMode::ASYNC,    
            GrabMode::ASYNC,    
            x11rb::NONE,        
            x11rb::NONE,        
            ButtonIndex::M1,    
            ModMask::ANY,       
        )?;

        self.state.conn.flush()?;
        
        loop {
            let ev = self.state.conn.wait_for_event()?;
            
            #[cfg(debug_assertions)] log::info!("Event: {:#?}", ev);

           if let Err(e) = self.state.handle_event(ev) {
                error!("{}", e);
           };
        }

        //Ok(())
    }

    fn prepare_log(log_path: &str) {
        fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!(
                    "[{} {}] {}",
                    record.level(),
                    record.target(),
                    message
                ))
            })
            .level(log::LevelFilter::Info)
            .chain(fern::log_file(log_path)
                .expect("Failed to open log file"))
.apply()
            .expect("Failed to initialize logger");
    } 
}

pub struct YATState<C: Connection> {
    conn: C,
    windows: IndexMap<u32, YATWindow>,
    scr_num: usize,
    screen: Screen,
    inpct: InputCt,
    focus_new: bool, 
}

impl<C: Connection> YATState<C> {
    pub fn new(conn: C, scr_num: usize, cfg: &Config) -> YATState<C> {
        let scr = conn.setup().roots[scr_num].clone();

        YATState { 
            conn: conn, 
            windows: IndexMap::new(),
            scr_num: scr_num,
            screen: scr,
            inpct: InputCt::new(cfg.general.sh.clone()),
            focus_new: cfg.general.focus_new.unwrap_or(true),
        }
    }

    fn handle_event(&mut self, ev: Event) 
            -> Result<(), Box<dyn std::error::Error>> {
        match ev {
            Event::ConfigureRequest(e) => {
                let aux = ConfigureWindowAux::from_configure_request(&e);

                self.conn.configure_window(e.window, &aux)?;
                self.conn.flush()?;
            }
            Event::MapRequest(e) => {
                let (x, y, w, h) = self.calc_cords(1)?;
                let new_win = YATWindow::new(
                    e.window, x, y
                );
                
                self.conn.configure_window(
                    e.window,
                    &ConfigureWindowAux::new()
                        .x(x as i32)
                        .y(y as i32)
                        .width(w)
                        .height(h)
                )?;
                self.windows.insert(e.window, new_win);

                self.conn.map_window(e.window)?;

                // if the window is only one or focus_new, focus it
                if self.windows.len() == 1 || self.focus_new {
                    self.conn.set_input_focus(
                        InputFocus::PARENT, 
                        e.window, 
                        x11rb::CURRENT_TIME
                    )?;
                }

                self.conn.flush()?;
            }
            Event::UnmapNotify(e) => {
                self.windows.remove(&e.window);
                self.update_all_sizes(0)?; // already removed thus 0 

                // if only one window left, focus it
                if self.windows.len() == 1 && let Some(w) 
                    = self.windows.get_index(0) {

                    self.conn.set_input_focus(
                        InputFocus::PARENT,
                        w.1.id, 
                        x11rb::CURRENT_TIME
                    )?;
                    self.conn.flush()?;
                }
            }
            Event::KeyRelease(e) => {
                info!("Key released: {:?}+{}", e.state, e.detail); 
                let ks_opt = keycode_to_keysym(&self.conn, e.detail);
                let mods: ModMask = ModMask::from(u16::from(e.state));
           
                if let Some(ks) = ks_opt {
                    let cut = Keycut::new(ks.into(), mods);
                    
                    if let Some(ae) = self.inpct.run_cut(cut) {
                        // execute action if it was actioncut 
                        self.exec_action(&ae)?;
                    };
                }
            }
            Event::ButtonPress(e) => {
                // left mouse button 
                if e.detail == 1 {
                    let win_id = e.child;

                    self.conn.set_input_focus(
                        InputFocus::PARENT, 
                        win_id,
                        x11rb::CURRENT_TIME,
                    )?;

                    self.conn.flush()?;
                } 
            }
            other => {
                // TODO
            }
        }
        Ok(())
    }

    /// Register shortcuts from config
    fn reg_scuts(&mut self, cfg: &Config) {
        let mainmod = match cfg.general.mainmod.to_lowercase().as_str() {
            "super" | "win" => ModMask::M4,
            "alt" => ModMask::M1,
            "shift" => ModMask::SHIFT,
            "ctrl" | "control" => ModMask::CONTROL,
            other => {
                error!("Unknown mainmod {}", other);
                return;
            }
        };

        for (key, val) in &cfg.shortcuts {
            let mut success = true;

            let (sym, keycode, modifiers) = 
                Self::parse_keyscomb(
                    key, 
                    mainmod, 
                    &self.conn
                ).unwrap_or_else(|| {
                    error!("Failed to parse shortcut {}", key);
                    success = false;
                    (xkb::Keysym(0), 0, ModMask::default())
                });
            if !success {continue;}

            // TODO: maybe check result
            let _ = self.conn.grab_key(
                        true, 
                        self.screen.root, 
                        modifiers,
                        keycode, 
                        GrabMode::ASYNC, 
                        GrabMode::ASYNC
            );

            let cut = Keycut::new(sym.into(), modifiers);

            let task = match val {
                ActionEnum::Command(c) => input::CutTask::Command(c.clone()),
                other => input::CutTask::Action(other.clone())
            };

            self.inpct.add_shortcut(
                cut, 
                task
            );
        }

        let _ = self.conn.flush();
    }

    /// Parses key combination and returns keysym, keycode  and modmask 
    fn parse_keyscomb(key: &str, mainmod: ModMask, conn: &C) 
        -> Option<(xkb::Keysym, u8, ModMask)> {
        let keys_iter = key.split('+');
    
        let mut modifiers = ModMask::default();
        let mut mkey: Option<xkb::Keysym> = None;
        let mut kcode: Option<u8> = None;
        let mut success: bool = true;

        for kst in keys_iter {
            match kst.to_lowercase().as_str() {
                "super" | "win" => {
                    modifiers |= ModMask::M4;    
                }
                "alt" => {
                    modifiers |= ModMask::M1;
                }
                "shift" => {
                    modifiers |= ModMask::SHIFT;
                }
                "ctrl" | "control" => {
                    modifiers |= ModMask::CONTROL;
                }
                "mod" => {
                    modifiers |= mainmod;
                }
                other => {
                    let sym = xkb::Keysym::from_str(other)
                        .unwrap_or_else(|_| {
                            error!("Unable to get keysym from {}", other);
                            success = false;
                            xkb::Keysym(0)
                        });
                    let keycode = keysym_to_keycode(&conn, sym)
                        .unwrap_or_else(|| {
                            error!("Unable to get keycode from \
                                keysym {} ({})", sym, other);
                            success = false;
                            0
                        });            
                    if !success {break;}

                    mkey = Some(sym);
                    kcode = Some(keycode);
                }
            }
        }

        let keycode = match kcode {
            Some(v) => v,
            None => {
                error!("CFGPARSE: Can't get keycode");
                return None;
            }
        };

        let sym = match mkey {
            Some(v) => v,
            None => {
                error!("CFGPARSE: shortcut must have a key");
                return None;
            }
        };
        Some((sym, keycode, modifiers))
    }

    fn exec_action(&mut self, action: &ActionEnum) 
        -> Result<(), Box<dyn std::error::Error>> {
        
        match action {
            ActionEnum::DummyAction(v) => {
                warn!("Dummy action made: {}", v);
            }
            other => {
                warn!("TODO: {:?}", other);
            }
        }
        
        Ok(())
    }
    
    /// Calculate pos and size of new window (posx, posy, sizex, sizey)
    /// Alg is straightforward: split in vertical bars basically
    fn calc_cords(&mut self, delta: i16) -> 
            Result<(u32, u32, u32, u32), Box<dyn std::error::Error>> { 
        let scr_height = self.screen.height_in_pixels;
        let scr_width = self.screen.width_in_pixels;
    
        let wind_width = self.update_all_sizes(delta)?; 

        Ok((
            (scr_width - wind_width).into(), 
            0, 
            wind_width.into(), 
            scr_height.into()
        ))
    }

    /// Updates all windows sizes and returns new window width
    fn update_all_sizes(&mut self, delta: i16) 
        -> Result<u16, Box<dyn std::error::Error>> {
         let ctr = self.windows.len();

        let scr_height = self.screen.height_in_pixels;
        let scr_width = self.screen.width_in_pixels;
    
        let wind_width = if (ctr as i16 + delta) == 0 {
            scr_width
        } else {
            scr_width / (ctr as i16 + delta) as u16
        };
        
        for (i, wind) in self.windows.values_mut().enumerate() {
            let new_x = i * wind_width as usize;
            
            self.conn.configure_window(
                    wind.id,
                    &ConfigureWindowAux::new()
                        .x(new_x as i32)
                        .y(wind.y as i32)
                        .width(wind_width as u32)
                        .height(scr_height as u32)
            )?;

            wind.x = new_x as u32;
        }
        
        self.conn.flush()?;
        Ok(wind_width)
    }
}

pub struct YATWindow {
    pub id: u32,
    pub x: u32,
    pub y: u32,
}

impl YATWindow {
    pub fn new(id: u32, x: u32, y: u32) -> YATWindow {
        YATWindow { id, x, y }
    }
}

pub fn get_homedpath(append: &str, cleanup: bool) -> Result<String, ()> {
    if let Some(path) = std::env::home_dir() {
        let res = format!("{}", path
                .join(PathBuf::from(append))
                .display()
        );
        
        // open file to cleanup it from last session (useful for debug)
        if cleanup && cfg!(debug_assertions)
        {
            std::fs::File::create(&res);
        }
        Ok(res)
    } else {
        Err(())
    }
}

#[derive(Debug)]
struct CustomError {
    message: String,
}
