# b5_mods_rs

Rust mods for Burnout 5 (`Burnout_PC.exe` built from
[b5-decomp](https://github.com/BurnoutDecomp/b5-decomp)), as sandboxed **WebAssembly** for the
game's mod host (`src/pc/mods/`, API in `src/pc/mods/api/brn_mod_api.h`, branch `feat/pc-mods`).
No DLL, no injection, no hooks: the game loads `mods/<name>/mod.ini` and the `.wasm` it names,
and the mod sees nothing but its own memory and the host's `brn.*` imports. One `.wasm` runs on
every OS/CPU the game is built for.

- `b5_mod_sdk` — the Rust side of the API: the `Mod` trait, `export_mod!`, typed services
  (player, vehicles, locations, race cars, traffic, the game's debug menu, `config.ini [Debug]`,
  the mod's own storage) and an egui overlay that hands its meshes to the host as draw lists.
- `debug_menu` — an example mod: a debug menu for testing the decomp in game (Insert).
  Vehicle (car change, infinite boost, invulnerability), teleport, motion, stats, traffic,
  race cars, the mod list, and the game's whole CgsDev debug menu in an egui window.

The same API from C++ with Dear ImGui: [b5_mods_cpp](https://github.com/macadelic80/b5_mods_cpp).

## Build and install

```bat
rustup target add wasm32-unknown-unknown
install_mods.bat <game folder>
```

builds in release (`.cargo/config.toml` targets `wasm32-unknown-unknown`) and copies
`debug_menu.wasm` + `mod.ini` into `<game folder>\mods\debug_menu\`. The mod's files
(`positions.txt`, `favourites.txt`) live in `mods\debug_menu\data\` — the only folder it can
reach; the script copies there the ones the DLL version left next to `mod.ini`.

A panic is written to `BrnGame.log` and switches the mod off (the game keeps running). Switching a
wasm mod back on from a Mods tab (another mod's, or `mods\mods.ini` before the next start) reloads
its `.wasm`, so a rebuild + toggle is a hot reload.
