v0.0.3:
- shortcuts handling
- parsing shortcuts from config, e.g.:
```toml
[general]
mainmod = "super"

[shortcuts]
"mod+t" = "xterm"
```
- new config var in `general` table: `sh`. String, determines prefed shell.
E.g.:
```toml
[general]
...
sh = "zsh"
```
- small code cleanse (need more though?)
