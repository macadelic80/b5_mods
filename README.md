# b5_mods

Rust mods for Burnout 5 (`Burnout_PC.exe` built from
[b5-decomp](https://github.com/BurnoutDecomp/b5-decomp)), using the game's native mod host
(`src/pc/mods/api/brn_mod_api.h`, branch `feat/pc-mods`). No DLL injection, no hooks: the
game loads `mods/<name>/mod.ini` and the DLL it names.

- `b5_mod_sdk` — the Rust side of the C API: the `Mod` trait, `export_mod!`, typed services
  (player, vehicles, locations, race cars, traffic, the game's debug menu) and an egui
  overlay drawn with the game's own D3D9 device.
- `debug_menu` — an example mod: a debug menu for testing the decomp in game (Insert).
  Vehicle (car change, infinite boost, invulnerability), teleport, motion, stats, traffic,
  race cars, the mod list, and the game's whole CgsDev debug menu in an egui window.

## Build and install

```bat
install_mods.bat <game folder>
```

builds in release and copies `debug_menu.dll` + `mod.ini` into `<game folder>\mods\debug_menu\`.
