use crate::core::WM;

mod core;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1) == Some(&"--version".to_owned()) {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let mut wm = WM::new();
    wm.run()?;

    Ok(())
}
