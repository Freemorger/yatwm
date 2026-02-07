use std::{collections::HashMap, path::PathBuf, str::FromStr};

use indexmap::IndexMap;
use log::{error, info, warn};
use maplit::hashmap;
use x11rb::{COPY_DEPTH_FROM_PARENT, connection::Connection, protocol::{Event, xproto::{ButtonIndex, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, GrabMode, InputFocus, ModMask, Screen, WindowClass}}, rust_connection::RustConnection};

use crate::core::{
    cfgread::{ActionEnum, ActionValue, CfgMacro, Config, keycode_to_keysym, keysym_to_keycode}, input::{InputCt, KeyRange, Keycut}, workspaces::Workspace
};

pub mod cfgread;
pub mod input;
pub mod workspaces;

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

        if let Some(sv) = self.cfg.general.autostart.as_ref() {
            for cmd in sv {
                self.state.inpct.run_cmd(&cmd); 
            }
        };
        
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
    workspaces: HashMap<usize, Workspace>,
    cur_scr: usize, // index of cur workspace 
    screen: Screen,
    inpct: InputCt,
    focus_new: bool, 
    macros: HashMap<String, CfgMacro>,
}

impl<C: Connection> YATState<C> {
    pub fn new(conn: C, scr_num: usize, cfg: &Config) -> YATState<C> {
        let scr = conn.setup().roots[scr_num].clone();
        
        let mut workspaces = HashMap::new();
        let wrksps_ct = cfg.general.def_wrksp_ctr.unwrap_or(1);
        for i in 1..(wrksps_ct + 1) { 
            let workspace = Workspace::new(i);
            workspaces.insert(i, workspace);
        }

        YATState { 
            conn: conn, 
            cur_scr: 1,
            screen: scr,
            inpct: InputCt::new(cfg.general.sh.clone()),
            focus_new: cfg.general.focus_new.unwrap_or(true),
            workspaces: workspaces,
            macros: cfg.macros.clone().unwrap_or(HashMap::new()),
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
                let new_win = YATWindow::new(
                    e.window, 0, 0 // would be updated anyways
                );

                let cur_wrksp = self.workspaces
                    .get_mut(&self.cur_scr).ok_or(CustomError {
                        message: "Can't get cur workspace".to_owned()
                     })?;
                cur_wrksp.add_wind(e.window, new_win);
                let cur_wrksp_len = cur_wrksp.windows.len();

                let (x, y, w, h) = self.calc_cords(0)?;
                
                

                self.conn.configure_window(
                    e.window,
                    &ConfigureWindowAux::new()
                        .x(x as i32)
                        .y(y as i32)
                        .width(w)
                        .height(h)
                )?;

                let event_mask = EventMask::EXPOSURE
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::PROPERTY_CHANGE
                    | EventMask::BUTTON_PRESS    
                    | EventMask::BUTTON_RELEASE  
                    | EventMask::POINTER_MOTION  
                    | EventMask::ENTER_WINDOW   
                    | EventMask::LEAVE_WINDOW
                    | EventMask::FOCUS_CHANGE;   

                self.conn.change_window_attributes(
                    e.window,
                    &ChangeWindowAttributesAux::default()
                        .event_mask(event_mask)
                )?;

                self.conn.map_window(e.window)?;

                // if the window is only one or focus_new, focus it
                if cur_wrksp_len == 1 || self.focus_new {
                    self.conn.set_input_focus(
                        InputFocus::PARENT, 
                        e.window, 
                        x11rb::CURRENT_TIME
                    )?;
                }

                self.conn.flush()?;
            }
            Event::DestroyNotify(e) => {
                self.rm_any_wind(e.window);
                self.update_all_sizes(0)?; // already removed thus 0 

                let cur_wrksp = self.workspaces
                    .get_mut(&self.cur_scr).ok_or(CustomError {
                    message: "Can't get cur workspace".to_owned()
                 })?;

                // if only one window left, focus it
                if cur_wrksp.windows.len() == 1 && let Some(w) 
                =  cur_wrksp.windows.get_index(0) {

                    self.conn.set_input_focus(
                        InputFocus::PARENT,
                        w.1.id, 
                        x11rb::CURRENT_TIME
                    )?;
                    self.conn.flush()?;
                }
            }
            Event::KeyRelease(e) => {
                let ks_opt = keycode_to_keysym(&self.conn, e.detail);
                let mods: ModMask = ModMask::from(u16::from(e.state));
           
                if let Some(ks) = ks_opt {
                    info!("Key released: {:?}+{} ({})", e.state, e.detail, 
                        ks.utf8());
                    let cut = Keycut::new(Some(ks.into()), mods, KeyRange::None);

                    if let Some(ae) = self.inpct.run_cut(cut) {
                        self.exec_action(&ae, &ks.utf8())?;
                    } else {
                        if let Some(digit) = ks.utf8().parse::<u8>().ok() {
                            if (1..=9).contains(&digit) {
                                let num_cut = Keycut::new(None, mods,
                                    KeyRange::Numbers);
                                if let Some(ae) = self.inpct.run_cut(num_cut) {
                                    self.exec_action(&ae, &ks.utf8())?;
                                    return Ok(());
                                }
                            }
                        }
                        
                        let any_cut = Keycut::new(None, mods, KeyRange::Any);
                        if let Some(ae) = self.inpct.run_cut(any_cut) {
                            self.exec_action(&ae, &ks.utf8())?;
                        } 
                    }
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

    fn rm_any_wind(&mut self, idx: u32) -> Option<YATWindow> {
        for (i, wrksp) in self.workspaces.iter_mut() {
            if let Some(v) = wrksp.rm_wind(idx as u32) {
                return Some(v);
            }; 
        }
        None
    }

    fn change_workspace(&mut self, new_id: usize) 
        -> Result<(), Box<dyn std::error::Error>> {
        
        let cur_wrksp = self.workspaces.get(&self.cur_scr).ok_or(CustomError {
            message: "Failed to get cur workspace".to_owned()
        })?;

        for (i, wind) in &cur_wrksp.windows {
            self.conn.unmap_window(wind.id)?;
        }
        
        match self.workspaces.get(&new_id) {
            Some(v) => {
                info!("found workpace {}, win len: {}", new_id, v.windows.len());
                for (i, wind) in &v.windows {
                    self.conn.map_window(wind.id)?;
                    if v.windows.len() == 1 {
                        self.conn.set_input_focus(
                            InputFocus::PARENT,
                            wind.id,
                            x11rb::CURRENT_TIME
                        )?;
                    }
                    info!("mapping wind {}", wind.id);
                }
            }
            None => {
                let wrksp = Workspace::new(new_id);
                self.workspaces.insert(new_id, wrksp);
                warn!("creating new workspace");
            }
        };

        self.cur_scr = new_id;
        self.update_all_sizes(0)?;
        self.conn.flush()?;

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

            let (sym, keycode, modifiers, krange) = 
                self.parse_keyscomb(
                    key, 
                    mainmod 
                ).unwrap_or_else(|| {
                    error!("Failed to parse shortcut {}", key);
                    success = false;
                    (xkb::Keysym(0), 0, ModMask::default(), KeyRange::None)
                });
            if !success {continue;}

            let mut cut = Keycut::new(
                None, 
                ModMask::default(), 
                KeyRange::None
            );

            match krange {
                KeyRange::Numbers => {
                    for i in 1..10 {
                        let keysym = match xkb::Keysym::from_str(&i.to_string()) {
                            Ok(keysym) => keysym,
                            Err(err) => {
                                error!("Warning: Failed to parse keysym\
                                    from '{}'", i);
                                continue;
                            }
                        };

                        let kc = match keysym_to_keycode(&self.conn, keysym) {
                            Some(kc) => kc,
                            None => {
                                error!("Warning: Can't get keycode for {:?}",
                                    keysym);
                                continue;
                            }
                        };

                        let _ = self.conn.grab_key(
                            true, 
                            self.screen.root, 
                            modifiers,
                            kc, 
                            GrabMode::ASYNC, 
                            GrabMode::ASYNC
                        );
                    }
                    cut = Keycut::new(
                        None,
                        modifiers,
                        KeyRange::Numbers
                    );
                }
                KeyRange::Any => {
                    let _ = self.conn.grab_key(
                        true, 
                        self.screen.root, 
                        modifiers,
                        0, 
                        GrabMode::ASYNC, 
                        GrabMode::ASYNC
                    );
                    cut = Keycut::new(
                        None,
                        modifiers,
                        KeyRange::Any
                    );
                }
                other => {
                    let _ = self.conn.grab_key(
                        true, 
                        self.screen.root, 
                        modifiers,
                        keycode, 
                        GrabMode::ASYNC, 
                        GrabMode::ASYNC
                    );
                    cut = Keycut::new(
                        Some(sym.into()),
                        modifiers,
                        KeyRange::None
                    );
                }
            }

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

    /// Attempts to expand `Define` macro
    fn try_macroexp(&mut self, name: &str) -> Option<String> {
        if let Some(CfgMacro::Define(expanded)) = self.macros.get(name) {
            return Some(expanded.clone());
        } else {
            return None;
        }
    }

    /// Parses key combination and returns keysym, keycode  and modmask 
    fn parse_keyscomb(&mut self, key: &str, mainmod: ModMask) 
        -> Option<(xkb::Keysym, u8, ModMask, KeyRange)> {
        let mut preproced = String::new();
        for k in key.split('+') {
            if k.starts_with('{') {
                let cleaned = k
                        .replace("{", "")
                        .replace("}", "");

                let mut err = false;
                let exp = self.try_macroexp(&cleaned).unwrap_or_else(|| {
                    error!("Unknown macro {}", cleaned);
                    err = true;
                    k.to_owned()
                });
                if err {return None;}

                preproced += &format!("{}+", exp);
            } else {
                preproced += &format!("{}+", k);
            }
        }
        if preproced.ends_with("+") {preproced.pop();}

        let keys_iter = preproced.split('+');
    
        let mut modifiers = ModMask::default();
        let mut mkey: Option<xkb::Keysym> = None;
        let mut kcode: Option<u8> = None;
        let mut success: bool = true;
        let mut krange = KeyRange::None;

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
                "|wildcard|" | "|anykey|" => {
                    krange = KeyRange::Any;
                }
                "|number|" | "|anynum|" => {
                    krange = KeyRange::Numbers;
                }
                other => {
                    let sym = xkb::Keysym::from_str(other)
                        .unwrap_or_else(|_| {
                            error!("Unable to get keysym from {}", other);
                            success = false;
                            xkb::Keysym(0)
                        });
                    let keycode = keysym_to_keycode(&self.conn, sym)
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
                if matches!(krange, KeyRange::Any) || matches!(krange, KeyRange::Numbers) {
                    0
                } else {
                    error!("CFGPARSE: Can't get keycode");
                    return None;
                }
            }
        };

        let sym = match mkey {
            Some(v) => v,
            None => {
                if matches!(krange, KeyRange::Any) || matches!(krange, KeyRange::Numbers) {
                    xkb::Keysym(0)
                } else {
                    error!("CFGPARSE: Shortcut must have a key or a placeholder");
                    return None;
                }       
            }
        };
        Some((sym, keycode, modifiers, krange))
    }

    /// Executes action. `addi` is additional key used for macros
    fn exec_action(&mut self, action: &ActionEnum, addi: &str) 
        -> Result<(), Box<dyn std::error::Error>> {
        
        match action {
            ActionEnum::SwitchWorkspace(id) => {
                self.change_workspace(*id)?;
            }
            ActionEnum::DeltaWorkspace(delta) => {
                let new_id = self.cur_scr.saturating_add_signed(*delta);

                if self.workspaces.get(&new_id).is_none() {
                    return Err(Box::new(CustomError {
                        message: format!("No workspace {}", new_id)
                    }));
                }

                self.change_workspace(new_id)?;
            }
            ActionEnum::MoveToWorkspace(new_id) => {
                let focused_id = self.conn.get_input_focus()?
                    .reply()?
                    .focus;
                info!("moving wind {} to {}-th workspace, cur: {}",
                    focused_id, new_id, self.cur_scr);

                // lets check if there is a new workspace 
                // so we dont just remove window in case of some trouble
                self.workspaces 
                    .get(new_id)
                    .ok_or(CustomError {
                        message: format!("Can't get current ({}) workspace",
                                     self.cur_scr)
                    })?;

                
                let cur_wrksp = self.workspaces
                    .get_mut(&self.cur_scr)
                    .ok_or(CustomError {
                        message: format!("Can't get current ({}) workspace",
                                     self.cur_scr)
                    })?;
                let removed = cur_wrksp.windows.shift_remove(&focused_id)
                    .ok_or(CustomError {
                    message: format!("Seems like window {} isn't in \
                                 current workspace!", focused_id) 
                })?;
                
                let new_worksp = self.workspaces 
                    .get_mut(new_id)
                    .ok_or(CustomError {
                        message: format!("Can't get current ({}) workspace",
                                     self.cur_scr)
                    })?;
                self.conn.unmap_window(removed.id)?;

                new_worksp.windows.insert(focused_id, removed);

                self.update_all_sizes(0)?;
            }
            ActionEnum::FocusOther(delta) => {
                let focus_id = self.conn.get_input_focus()?
                    .reply()?
                    .focus;

                let cur_wrksp = self.workspaces.get(&self.cur_scr)
                    .ok_or(CustomError {
                        message: format!("Can't get cur workspace {}",
                                     self.cur_scr)
                    })?;

                let idx = cur_wrksp.windows.get_index_of(&focus_id)
                    .ok_or(CustomError {
                        message: format!("Can't get window {} in cur workspace",
                                     focus_id)
                    })?;

                let new_id = idx.saturating_add_signed(*delta);

                let mut flag = false;
                let placehold = YATWindow::new(0,0,0);

                let new_focus = cur_wrksp.windows.get_index(new_id)
                    .unwrap_or_else(|| {
                        warn!("Trying to switch to non-existent {}-th window",
                            new_id);
                        flag = true;
                        (&0, &placehold)
                    });
                if flag {return Ok(());} // avoiding errors spam

                self.conn.set_input_focus(
                    InputFocus::PARENT, 
                    *new_focus.0,
                    x11rb::CURRENT_TIME
                )?;

                self.conn.flush()?;
            }
            ActionEnum::CfgReload(_) => {
                self.reload_cfg()?;
            }
            ActionEnum::ExpandMacro(name, arg) => {
                // TODO: remove clone here for better perf 
                let macros = self.macros.get(name).ok_or(CustomError {
                    message: format!("Can't get macro {}", name)
                })?.clone();

                match macros {
                    CfgMacro::DefineActions(v) => {
                        for act in v.iter() {
                            self.exec_action(act, addi)?;
                        }
                    }
                    CfgMacro::ReplaceAll(v, a) => {
                        let exp_arg = if matches!(arg, ActionValue::GetFromCut) {
                            if addi.chars().all(char::is_numeric) {
                                ActionValue::Uint(addi.parse()?)
                            } else {
                                ActionValue::Str(addi.to_string())
                            }
                        } else {
                            arg.clone()
                        };

                        if std::mem::discriminant(&exp_arg) != 
                            std::mem::discriminant(&a) {
                            return Err(Box::new(CustomError {
                                message: format!(
                                "Type error: passed {:?} but {:?} was expected",
                                arg, a)}));
                        }

                        let new = exp_arg.clone();
                        for act in v.iter() {
                            let changed = act.replace_val(&new)?.clone();
                            self.exec_action(&changed, addi)?;
                        } 
                    }
                    other => {
                        error!("Unimplemnted macro expand {:?}", other);
                    }
                } 

            }
            ActionEnum::Complex(v) => {
                for act in v {
                    self.exec_action(act, addi)?;
                }
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

    /// Updates all windows sizes and returns new window width except `except`
    fn update_all_sizes(&mut self, delta: i16) 
        -> Result<u16, Box<dyn std::error::Error>> {
        let cur_wrksp = self.workspaces
            .get_mut(&self.cur_scr)
            .ok_or(CustomError {
                message: "Can't get cur workspace".to_owned()}
            )?;
        let ctr = cur_wrksp.windows.len();

        let scr_height = self.screen.height_in_pixels;
        let scr_width = self.screen.width_in_pixels;
    
        let wind_width = if (ctr as i16 + delta) == 0 {
            scr_width
        } else {
            scr_width / (ctr as i16 + delta) as u16
        };
        
        for (i, wind) in cur_wrksp.windows.values_mut().enumerate() {
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

    fn reload_cfg(&mut self) -> 
        Result<(), Box<dyn std::error::Error>> {

        let new_cfg = Config::from_def_dir();
        self.focus_new = new_cfg.general.focus_new.unwrap_or(true);
        self.inpct.shell = new_cfg.general.sh
            .clone()
            .unwrap_or("sh".to_owned());

        self.macros.clear();
        self.macros.extend(
            new_cfg.macros
                .clone()
                .unwrap_or(HashMap::new())
                .into_iter()
        );

        self.conn.ungrab_key(
            0, // any key  
            self.screen.root, 
            ModMask::ANY
        )?;

        self.reg_scuts(&new_cfg);
        self.conn.flush()?;
        
        Ok(())
    }
}

#[derive(Debug)]
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

impl std::fmt::Display for CustomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CustomError {
}
