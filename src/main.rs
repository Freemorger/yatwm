use crate::core::WM;

mod core;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut wm = WM::new();
    wm.run()?;

    Ok(())
}
