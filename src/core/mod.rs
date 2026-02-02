use std::{collections::HashMap, path::PathBuf};

use indexmap::IndexMap;
use log::{error, info};
use x11rb::{COPY_DEPTH_FROM_PARENT, connection::Connection, protocol::{Event, xproto::{ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, Screen, WindowClass}}, rust_connection::RustConnection};

use crate::core::cfgread::Config;

pub mod cfgread;

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

        WM {
            cfg: cfg,
            state: YATState::new(conn, scr_num)
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.state.conn.change_window_attributes(
            self.state.screen.root,
            &ChangeWindowAttributesAux::new().event_mask(
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY
            ),
        )?;

        self.state.conn.flush();
        
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
}

impl<C: Connection> YATState<C> {
    pub fn new(conn: C, scr_num: usize) -> YATState<C> {
        let scr = conn.setup().roots[scr_num].clone();

        YATState { 
            conn: conn, 
            windows: IndexMap::new(),
            scr_num: scr_num,
            screen: scr,
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

                self.conn.map_window(e.window)?;

                self.conn.flush()?;

                self.windows.insert(e.window, new_win);
            }
            Event::UnmapNotify(e) => {
                self.windows.remove(&e.window);
                self.update_all_sizes(0)?; // already removed
            }            
            other => {
                // TODO
            }
        }
        Ok(())
    }

    /// Calculate pos and size of new window (posx, posy, sizex, sizey)
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
