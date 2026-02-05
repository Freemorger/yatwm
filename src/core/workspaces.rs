use indexmap::IndexMap;

use crate::core::YATWindow;


#[derive(Debug)]
pub struct Workspace {
    pub idx: usize, 
    pub windows: IndexMap<u32, YATWindow>,
}

impl Workspace {
    pub fn new(idx: usize) -> Workspace {
        Workspace {
            idx: idx, 
            windows: IndexMap::new(), 
        }
    }

    pub fn add_wind(&mut self, idx: u32, wind: YATWindow) {
        self.windows.insert(idx, wind);
    }

    // remove window
    pub fn rm_wind(&mut self, idx: u32) -> Option<YATWindow> {
        self.windows.remove(&idx)
    }
}
