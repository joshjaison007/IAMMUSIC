<div align="center">

# IAMMUSIC
### Ultra-Lightweight Modern Desktop HUD Media Player for Windows

[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011-0078D6?style=for-the-badge&logo=windows&logoColor=white)](https://github.com)
[![Language](https://img.shields.io/badge/Language-Rust%202021-DEA584?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![UI Framework](https://img.shields.io/badge/UI-Slint%20(FemtoVG%20GPU)-2379F4?style=for-the-badge)](https://slint.dev)
[![Memory](https://img.shields.io/badge/Memory-%3C%2010%20MB%20RAM-success?style=for-the-badge)](#-resource-consumption--performance)
[![License](https://img.shields.io/badge/License-MIT-yellow?style=for-the-badge)](license.txt)

<br/>

**IAMMUSIC** is an ultra-minimalist, high-performance desktop HUD widget built to blend effortlessly into clean desktop environments, custom rainmeter setups, and gaming rigs. Powered by native **Rust** and hardware-accelerated **Slint (FemtoVG)**, it interfaces directly with Windows System Media Transport Controls (SMTC) to deliver real-time track metadata, fluid vinyl disc animations, and gamepad/keyboard controls.

<br/>

[Key Features](#-features) • [Performance & RAM](#-resource-consumption--performance) • [What Changed](#-evolution--recent-updates) • [Installation](#-installation) • [Controls & Keybinds](#-controls--hotkeys) • [Contact](#-contact--support)

---

</div>

## Resource Consumption & Performance

IAMMUSIC was engineered from the ground up for extreme efficiency. Unlike Electron, Chromium, or interpreted Python widgets that easily consume **150 MB – 300 MB** of RAM, IAMMUSIC is compiled directly to native x86_64 machine code with aggressive kernel-level memory management.

```
┌───────────────────────────────────────────────────────────────┐
│                     RESOURCE FOOTPRINT                        │
├────────────────────────┬──────────────────────────────────────┤
│ Working Set RAM        │ ~1.2 MB – 4.5 MB (Strictly < 10 MB)  │
│ CPU Usage (Playback)   │ ~0.4% – 1.0%                         │
│ CPU Usage (Idle/Pause) │ ~0.0% – 0.1%                         │
│ Disk / Network I/O     │ 0.0 MB/s                             │
└────────────────────────┴──────────────────────────────────────┘
```

### How It Stays Under 10 MB:
- **Direct Kernel Page Trimming**: Integrates native Windows APIs `EmptyWorkingSet()` and `HeapCompact()` to immediately purge transient render caches and decommit unused heap pages back to the OS.
- **1:1 Native Texture Sizing**: Decodes and resizes album art directly to the native $106 \times 106\text{px}$ screen disc footprint in memory, slashing GPU texture mapping buffers by **75%**.
- **Zero Heap Allocations on Timers**: The 20 FPS vinyl rotation loop and marquee text scroller operate entirely on atomic integers and primitive floats with zero dynamic memory churn.
- **Zero-Unwind Release Profile**: Compiled with `panic = "abort"` and dead-code stripping, eliminating landing pads, exception tables, and unnecessary runtime bloat.

---

## Evolution & Recent Updates

This project originally started as an experimental Python prototype and was completely rewritten in **Rust** for production stability, visual finesse, and minimal resource usage:

| Feature / Metric | Legacy Python Version | **IAMMUSIC (Current Rust Edition)** |
| :--- | :--- | :--- |
| **Runtime & Dependencies** | Requires Python 3.10+ & pip packages | **Zero Dependencies** (Self-contained native `.exe`) |
| **RAM Footprint** | ~50 MB – 85 MB | **~1.2 MB – 4.5 MB (< 10 MB guaranteed)** |
| **Renderer** | Software Canvas / Tkinter | **FemtoVG Hardware-Accelerated GPU** |
| **Album Art Quality** | Unclipped square edges | **Mathematical anti-aliased circular masking** |
| **Window Frame** | Flashed white caption bar when inactive | **True borderless Win32 non-client suppression** |
| **Keybind Support** | Fixed basic keys | **Independent Gamepad (XInput) & Keyboard sections** |
| **Customization** | Hardcoded colors | **Adaptive drawer, hex inputs & native palette picker** |
| **Installation** | Manual terminal / pip setup | **1-Click Windows Setup (`IAMMUSIC_Installer.exe`)** |

---

## Features

- **Vinyl Album Art & Smooth Momentum**:
  - Automatically hooks into Spotify, Apple Music, Tidal, YouTube Music, Chrome, Firefox, and Discord media streams.
  - Generates a circular vinyl record platter with realistic acceleration and deceleration momentum during playback.
  - When playback stops or rotation is toggled off, the disc automatically eases back to a clean upright $0^\circ$ position.
- **Dynamic Capsule Controls**:
  - Responsive center capsule pill button with subtle outward nudge reactions on the Previous and Next track buttons.
  - Custom ripple feedback animations on clicks and hover states.
- **Scroll-to-Volume**:
  - Hover over the album platter and scroll your mouse wheel to adjust system master volume with a floating HUD indicator (`VOL ▲` / `VOL ▼`).
- **Adaptive HUD Customization Drawer**:
  - Slides out adaptively to the left or right depending on screen edge proximity.
  - Pre-curated high-contrast accent and background themes.
  - Custom Hex input (`#RRGGBB` or `#RGB`) + native Windows Color Picker integration with 4-slot palette memory.
  - Window transparency toggles ($100\%$, $75\%$, $50\%$) and Always-on-Top pinning.
- **Divided Gamepad & Keyboard Sections**:
  - Clean separation between **Gamepad Controls** (Xbox / XInput) and **Keyboard Hotkeys** with dedicated toggle switches.
  - Collapsing either toggle hides its respective keybind list cleanly with dynamic drawer resizing (no awkward blank space).
  - One-click remapping: click `[PRESS PAD...]` or `[PRESS KEY...]` to capture any controller button, trigger, or keyboard shortcut (with Shift, Ctrl, and Alt modifiers).
- **Stealth Frameless Architecture**:
  - Employs a custom Win32 subclass that intercepts `WM_NCACTIVATE`, `WM_NCPAINT`, and `WM_NCCALCSIZE`.
  - Disables DWM non-client rendering policy (`DWMNCRP_DISABLED`), guaranteeing zero white title bar or container border flashes when switching tabs or clicking outside the widget.
- **System Tray Integration**:
  - Sits quietly in the notification tray with customized tooltip status and quick-exit options.

---

## Installation

### Option 1: Windows Installer (Recommended)
1. Download **`IAMMUSIC_Installer.exe`** from the [github release](github%20release/) folder or GitHub Releases.
2. Run the setup wizard to install IAMMUSIC to your system with start menu shortcuts and clean uninstallation support.

### Option 2: Portable Standalone Executable
1. Download **`IAMMUSIC.exe`**.
2. Run it directly from any folder, USB drive, or auto-start directory—no installation or external runtimes required.

---

## Controls & Hotkeys (work in progress)

### Default Mappings

| Action | Gamepad (Xbox / XInput) | Keyboard Shortcut | Mouse Interaction |
| :--- | :--- | :--- | :--- |
| **Play / Pause** | `Pad A` | `Space` | Click center capsule pill |
| **Next Track** | `Pad RB` | `Right Arrow` | Click Next circular button |
| **Previous Track** | `Pad LB` | `Left Arrow` | Click Prev circular button |
| **Volume Up / Down** | — | — | `Scroll Wheel Up / Down` over Vinyl |
| **Move Widget** | — | — | `Click & Drag` background |
| **Settings Drawer** | — | — | Click Gear icon (top left) |
| **Exit** | — | — | Click `✕` (top right) or System Tray |

---

## Building from Source

### Prerequisites
- [Rust](https://rustup.rs/) (1.75+ recommended, MSVC target)
- *(Optional)* [Inno Setup 6](https://jrsoftware.org/isdl.php) if you wish to compile the `.exe` installer

### 1-Click PowerShell Build
```powershell
# Clone the repository
git clone https://github.com/yourusername/IAMMUSIC.git
cd IAMMUSIC

# Run the automated build script (compiles release binary & installer)
powershell.exe -ExecutionPolicy Bypass -File .\build.ps1
```

Or compile manually with Cargo:
```powershell
cd rust_app
cargo build --release
```
The resulting executable will be in `rust_app\target\release\iammusic.exe`.

---

## Contact & Support

This project is actively maintained and continuously evolving. If you encounter bugs, have feature suggestions, or need assistance, feel free to reach out:

- **Email**: [joshjaison2020@gmail.com](mailto:joshjaison2020@gmail.com)
- **Discord**: `spiketones007`

---

## License

This project is licensed under the MIT License — see the [license.txt](license.txt) file for details.
