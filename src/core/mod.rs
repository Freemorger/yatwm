use std::{collections::HashMap, path::PathBuf};

use log::error;
use x11rb::{COPY_DEPTH_FROM_PARENT, connection::Connection, protocol::{Event, xproto::{ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, CreateWindowAux, EventMask, WindowClass}}, rust_connection::RustConnection};

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
        let scr_num = self.state.scr_num;
        let screen = &self.state.conn.setup().roots[scr_num];

        self.state.conn.change_window_attributes(
            screen.root,
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
    windows: HashMap<u32, YATWindow>,
    scr_num: usize,
}

impl<C: Connection> YATState<C> {
    pub fn new(conn: C, scr_num: usize) -> YATState<C> {
        YATState { 
            conn: conn, 
            windows: HashMap::new(),
            scr_num: scr_num,
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
                let (x, y, w, h) = self.calc_cords();
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
            other => {
                // TODO
            }
        }
        Ok(())
    }

    /// Calculate pos and size of new window (posx, posy, sizex, sizey)
    fn calc_cords(&mut self) -> (u32, u32, u32, u32) { 
        // TODO 
        (0, 0, 400, 500)
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
