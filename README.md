## YATWM - Yet Another Tilling Window Manager
X11-based window manager for linux/freebsd. WIP. 
## Build 
Just a normal `cargo build`. `cargo build --release` if ur sure you need 
release build.  
### Debug enviroment for testing 
To run this in debug enviroment, install Xephyr and run script 
`rundebug.sh`. Ctrl+C stops it and prints `yatwm.log`  
## Debugging and configuring
`~/.local/state/yatwm.log` - WM logs    
`~/.config/yatwm/yat.toml` - put your config here, otherwise defaults 
will be loaded   
Config (WIP) example:
```toml
[general]
mainmod = "super"

[shortcuts]
"mod+t" = "xterm"
```
