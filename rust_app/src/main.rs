#![windows_subsystem = "windows"]

slint::include_modules!();

use slint::{
    Color, ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode,
};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::core::{w, HSTRING};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MEDIA_NEXT_TRACK,
    VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_VOLUME_DOWN, VK_VOLUME_UP,
};
use windows::Win32::UI::Shell::{
    DefSubclassProc, SetWindowSubclass, Shell_NotifyIconW, NIF_ICON, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetCursorPos, GetSystemMetrics, GetWindowLongW, LoadImageW, SendMessageW,
    SetForegroundWindow, SetWindowLongW, SetWindowPos, SetWindowTextW, ShowWindow, GWL_EXSTYLE,
    GWL_STYLE, HICON, HWND_TOPMOST, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTSIZE,
    LR_LOADFROMFILE, SM_CXSCREEN, SW_SHOW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    WM_SETICON, WS_EX_TOOLWINDOW,
};

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lp_mutex_attributes: *const std::ffi::c_void,
        b_initial_owner: windows::Win32::Foundation::BOOL,
        lp_name: windows::core::PCWSTR,
    ) -> windows::Win32::Foundation::HANDLE;

    fn SetProcessWorkingSetSize(
        h_process: windows::Win32::Foundation::HANDLE,
        dw_minimum_working_set_size: usize,
        dw_maximum_working_set_size: usize,
    ) -> windows::Win32::Foundation::BOOL;
}

#[link(name = "psapi")]
extern "system" {
    fn EmptyWorkingSet(h_process: windows::Win32::Foundation::HANDLE) -> windows::Win32::Foundation::BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetProcessHeap() -> windows::Win32::Foundation::HANDLE;
    fn HeapCompact(h_heap: windows::Win32::Foundation::HANDLE, dw_flags: u32) -> usize;
}

#[inline(always)]
fn trim_working_set() {
    unsafe {
        let _ = HeapCompact(GetProcessHeap(), 0);
        let _ = EmptyWorkingSet(GetCurrentProcess());
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

unsafe extern "system" fn frameless_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _ref_data: usize,
) -> windows::Win32::Foundation::LRESULT {
    match msg {
        // WM_NCPAINT (0x0085): Suppress Windows non-client frame and borders completely
        0x0085 => windows::Win32::Foundation::LRESULT(0),
        // WM_NCACTIVATE (0x0086): Return 1 to prevent DefWindowProc from painting the inactive caption bar
        0x0086 => windows::Win32::Foundation::LRESULT(1),
        // WM_NCCALCSIZE (0x0083): Return 0 so client area is 100% of window with zero non-client borders
        0x0083 => {
            if wparam.0 != 0 {
                windows::Win32::Foundation::LRESULT(0)
            } else {
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
        }
        // WM_ERASEBKGND (0x0014): Return 1 to eliminate white background painting flashes
        0x0014 => windows::Win32::Foundation::LRESULT(1),
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct XInputGamepad {
    w_buttons: u16,
    b_left_trigger: u8,
    b_right_trigger: u8,
    s_thumb_lx: i16,
    s_thumb_ly: i16,
    s_thumb_rx: i16,
    s_thumb_ry: i16,
}

#[repr(C)]
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct XInputState {
    dw_packet_number: u32,
    gamepad: XInputGamepad,
}

type PfnXInputGetState = unsafe extern "system" fn(u32, *mut XInputState) -> u32;

struct XInput {
    pfn: Option<PfnXInputGetState>,
}

impl XInput {
    fn load() -> Self {
        unsafe {
            use windows::core::w;
            use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
            for name in [w!("xinput1_4.dll"), w!("xinput1_3.dll"), w!("xinput9_1_0.dll")] {
                if let Ok(h) = LoadLibraryW(name) {
                    if !h.0.is_null() {
                        if let Some(proc) = GetProcAddress(h, windows::core::s!("XInputGetState")) {
                            let pfn: PfnXInputGetState = std::mem::transmute(proc);
                            return Self { pfn: Some(pfn) };
                        }
                    }
                }
            }
            Self { pfn: None }
        }
    }

    fn get_state(&self, user: u32) -> Option<XInputState> {
        let pfn = self.pfn?;
        let mut state = XInputState::default();
        unsafe {
            if pfn(user, &mut state) == 0 {
                Some(state)
            } else {
                None
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
enum BoundAction {
    Controller { buttons: u16, lt: bool, rt: bool, label: String },
    Keyboard { vk: i32, ctrl: bool, alt: bool, shift: bool, label: String },
    Combo { controller_buttons: u16, keyboard_vk: i32, label: String },
}

fn format_controller(buttons: u16, lt: bool, rt: bool) -> String {
    let mut parts = Vec::new();
    if buttons & 0x0100 != 0 { parts.push("Pad LB"); }
    if buttons & 0x0200 != 0 { parts.push("Pad RB"); }
    if lt { parts.push("Pad LT"); }
    if rt { parts.push("Pad RT"); }
    if buttons & 0x1000 != 0 { parts.push("Pad A"); }
    if buttons & 0x2000 != 0 { parts.push("Pad B"); }
    if buttons & 0x4000 != 0 { parts.push("Pad X"); }
    if buttons & 0x8000 != 0 { parts.push("Pad Y"); }
    if buttons & 0x0001 != 0 { parts.push("Pad Up"); }
    if buttons & 0x0002 != 0 { parts.push("Pad Down"); }
    if buttons & 0x0004 != 0 { parts.push("Pad Left"); }
    if buttons & 0x0008 != 0 { parts.push("Pad Right"); }
    if buttons & 0x0010 != 0 { parts.push("Pad Start"); }
    if buttons & 0x0020 != 0 { parts.push("Pad Back"); }
    if parts.is_empty() { "Pad Button".to_string() } else { parts.join("+") }
}

fn format_key(vk: i32, ctrl: bool, alt: bool, shift: bool) -> String {
    let mut parts = Vec::new();
    if ctrl { parts.push("Ctrl".to_string()); }
    if alt { parts.push("Alt".to_string()); }
    if shift { parts.push("Shift".to_string()); }
    let name = match vk {
        0x20 => "Space".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x0D => "Enter".to_string(),
        0x09 => "Tab".to_string(),
        0x1B => "Esc".to_string(),
        0x70..=0x7B => format!("F{}", vk - 0x70 + 1),
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0xB0 => "MediaNext".to_string(),
        0xB1 => "MediaPrev".to_string(),
        0xB3 => "MediaPlay".to_string(),
        _ => format!("Key{}", vk),
    };
    parts.push(name);
    parts.join("+")
}

#[repr(C)]
struct Win32ChooseColorW {
    l_struct_size: u32,
    hwnd_owner: HWND,
    h_instance: windows::Win32::Foundation::HINSTANCE,
    rgb_result: u32,
    lp_cust_colors: *mut u32,
    flags: u32,
    l_cust_data: isize,
    lpfn_hook: Option<unsafe extern "system" fn() -> usize>,
    lp_template_name: *const u16,
}

#[link(name = "comdlg32")]
extern "system" {
    fn ChooseColorW(lpcc: *mut Win32ChooseColorW) -> windows::Win32::Foundation::BOOL;
}

fn open_native_color_picker(hwnd: HWND, initial_color: Color) -> Option<Color> {
    unsafe {
        let mut custom_colors = [0xFFFFFFu32; 16];
        let init_rgb = (initial_color.red() as u32)
            | ((initial_color.green() as u32) << 8)
            | ((initial_color.blue() as u32) << 16);

        let mut cc = Win32ChooseColorW {
            l_struct_size: std::mem::size_of::<Win32ChooseColorW>() as u32,
            hwnd_owner: hwnd,
            h_instance: Default::default(),
            rgb_result: init_rgb,
            lp_cust_colors: custom_colors.as_mut_ptr(),
            flags: 0x00000001 | 0x00000002, // CC_RGBINIT | CC_FULLOPEN
            l_cust_data: 0,
            lpfn_hook: None,
            lp_template_name: std::ptr::null(),
        };

        if ChooseColorW(&mut cc).as_bool() {
            let r = (cc.rgb_result & 0xFF) as u8;
            let g = ((cc.rgb_result >> 8) & 0xFF) as u8;
            let b = ((cc.rgb_result >> 16) & 0xFF) as u8;
            Some(Color::from_rgb_u8(r, g, b))
        } else {
            None
        }
    }
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Color::from_rgb_u8(r, g, b))
    } else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
        Some(Color::from_rgb_u8(r, g, b))
    } else {
        None
    }
}

fn adjust_volume(up: bool) {
    unsafe {
        let vk = if up { VK_VOLUME_UP } else { VK_VOLUME_DOWN };
        keybd_event(vk.0 as u8, 0, KEYEVENTF_EXTENDEDKEY, 0);
        keybd_event(vk.0 as u8, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0);
    }
}

fn send_media_key(vk: VIRTUAL_KEY) {
    unsafe {
        keybd_event(vk.0 as u8, 0, Default::default(), 0);
        keybd_event(vk.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

struct MediaState {
    title: String,
    artist: String,
    is_playing: bool,
    thumbnail_bytes: Option<Vec<u8>>,
    thumbnail_version: u64,
}

fn fetch_media_state(
    manager: &Option<GlobalSystemMediaTransportControlsSessionManager>,
    last_known_title: &str,
    last_known_artist: &str,
    thumb_counter: &mut u64,
) -> Option<(
    MediaState,
    Option<GlobalSystemMediaTransportControlsSession>,
)> {
    let mgr = manager.as_ref()?;
    let mut session = mgr.GetCurrentSession().ok();

    if let Ok(sessions) = mgr.GetSessions() {
        let mut playing = None;
        let mut first = None;
        for s in sessions {
            if first.is_none() {
                first = Some(s.clone());
            }
            if let Ok(pb) = s.GetPlaybackInfo() {
                if let Ok(st) = pb.PlaybackStatus() {
                    if st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
                        playing = Some(s);
                        break;
                    }
                }
            }
        }
        if playing.is_some() {
            session = playing;
        } else if session.is_none() {
            session = first;
        }
    }

    let active_session = session?;
    let is_playing = active_session
        .GetPlaybackInfo()
        .ok()
        .and_then(|pb| pb.PlaybackStatus().ok())
        .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
        .unwrap_or(false);

    let props = active_session
        .TryGetMediaPropertiesAsync()
        .ok()?
        .get()
        .ok()?;
    let title = props
        .Title()
        .unwrap_or_else(|_| HSTRING::from(""))
        .to_string();
    let artist = props
        .Artist()
        .unwrap_or_else(|_| HSTRING::from(""))
        .to_string();

    let title_upper = if title.is_empty() {
        "NO MEDIA DETECTED".to_string()
    } else {
        title.to_uppercase()
    };

    // ONLY fetch thumbnail when song title/artist changes (stops constant 10MB heap churning)
    let track_changed = title_upper != last_known_title || artist != last_known_artist;
    let mut thumbnail_bytes = None;

    if track_changed {
        if let Ok(thumb_ref) = props.Thumbnail() {
            if let Ok(async_op) = thumb_ref.OpenReadAsync() {
                if let Ok(stream) = async_op.get() {
                    if let Ok(size) = stream.Size() {
                        if size > 0 && size < 10_000_000 {
                            if let Ok(input_stream) = stream.GetInputStreamAt(0) {
                                if let Ok(reader) = DataReader::CreateDataReader(&input_stream) {
                                    if let Ok(load_op) = reader.LoadAsync(size as u32) {
                                        if load_op.get().is_ok() {
                                            let mut buffer = vec![0u8; size as usize];
                                            if reader.ReadBytes(&mut buffer).is_ok() {
                                                thumbnail_bytes = Some(buffer);
                                                *thumb_counter += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Some((
        MediaState {
            title: title_upper,
            artist,
            is_playing,
            thumbnail_bytes,
            thumbnail_version: *thumb_counter,
        },
        Some(active_session),
    ))
}

fn update_custom_slot(app: &AppWindow, c: Color) {
    let c3 = app.get_custom_color_3();
    let c2 = app.get_custom_color_2();
    let c1 = app.get_custom_color_1();
    app.set_custom_color_4(c3);
    app.set_custom_color_3(c2);
    app.set_custom_color_2(c1);
    app.set_custom_color_1(c);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Single instance mutex: Prevent running multiple instances simultaneously in user session
    let _single_instance_mutex = unsafe {
        let mutex = CreateMutexW(
            std::ptr::null(),
            windows::Win32::Foundation::BOOL(1),
            w!("IAMMUSIC_SingleInstance_Mutex"),
        );
        if !mutex.0.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(hwnd) = FindWindowW(None, w!("IAMMUSIC")) {
                if !hwnd.0.is_null() {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE,
                    );
                }
            }
            return Ok(());
        }
        mutex
    };

    let app = AppWindow::new()?;
    let app_weak = app.as_weak();

    // Toggle states for Gamepad and Keyboard
    let enable_gamepad = Arc::new(AtomicBool::new(true));
    let enable_keyboard = Arc::new(AtomicBool::new(true));

    let eg_app = enable_gamepad.clone();
    let app_weak_eg = app_weak.clone();
    app.on_toggle_gamepad(move || {
        if let Some(app) = app_weak_eg.upgrade() {
            let cur = app.get_enable_gamepad();
            app.set_enable_gamepad(!cur);
            eg_app.store(!cur, Ordering::Relaxed);
        }
    });

    let ek_app = enable_keyboard.clone();
    let app_weak_ek = app_weak.clone();
    app.on_toggle_keyboard(move || {
        if let Some(app) = app_weak_ek.upgrade() {
            let cur = app.get_enable_keyboard();
            app.set_enable_keyboard(!cur);
            ek_app.store(!cur, Ordering::Relaxed);
        }
    });

    // Keybinds & Controller mappings:
    // Slots 0..=2: Gamepad (Play/Pause, Next, Prev)
    // Slots 3..=5: Keyboard (Play/Pause, Next, Prev)
    let bindings = Arc::new(Mutex::new([
        BoundAction::Controller { buttons: 0x1000, lt: false, rt: false, label: "Pad A".to_string() }, // Slot 0: Pad Play/Pause
        BoundAction::Controller { buttons: 0x0200, lt: false, rt: false, label: "Pad RB".to_string() }, // Slot 1: Pad Next
        BoundAction::Controller { buttons: 0x0100, lt: false, rt: false, label: "Pad LB".to_string() }, // Slot 2: Pad Prev
        BoundAction::Keyboard { vk: 0x20, ctrl: false, alt: false, shift: false, label: "Space".to_string() }, // Slot 3: Key Play/Pause
        BoundAction::Keyboard { vk: 0x27, ctrl: false, alt: false, shift: false, label: "Right".to_string() }, // Slot 4: Key Next
        BoundAction::Keyboard { vk: 0x25, ctrl: false, alt: false, shift: false, label: "Left".to_string() },  // Slot 5: Key Prev
    ]));

    let recording_slot = Arc::new(AtomicI32::new(-1));

    app.set_bind_pad_play_pause(SharedString::from("Pad A"));
    app.set_bind_pad_next(SharedString::from("Pad RB"));
    app.set_bind_pad_prev(SharedString::from("Pad LB"));
    app.set_bind_key_play_pause(SharedString::from("Space"));
    app.set_bind_key_next(SharedString::from("Right"));
    app.set_bind_key_prev(SharedString::from("Left"));
    app.set_recording_slot(-1);
    app.set_enable_gamepad(true);
    app.set_enable_keyboard(true);

    let rec_slot_cb = recording_slot.clone();
    let app_weak_rec = app_weak.clone();
    app.on_start_recording(move |slot| {
        rec_slot_cb.store(slot, Ordering::Relaxed);
        if let Some(app) = app_weak_rec.upgrade() {
            app.set_recording_slot(slot);
        }
    });

    // Background Controller & Global Keybind Thread
    let xinput = XInput::load();
    let bindings_input = bindings.clone();
    let recording_input = recording_slot.clone();
    let app_weak_input = app_weak.clone();
    let eg_input = enable_gamepad.clone();
    let ek_input = enable_keyboard.clone();

    std::thread::spawn(move || {
        let mut prev_buttons = 0u16;
        let mut prev_lt = false;
        let mut prev_rt = false;
        let mut prev_keys_down = [false; 256];

        loop {
            let rec_slot = recording_input.load(Ordering::Relaxed);
            let pad_on = eg_input.load(Ordering::Relaxed);
            let kbd_on = ek_input.load(Ordering::Relaxed);

            // Fast sleep if both toggles disabled and not recording
            if !pad_on && !kbd_on && rec_slot < 0 {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }

            let state = if pad_on || (rec_slot >= 0 && rec_slot <= 2) { xinput.get_state(0) } else { None };

            let ctrl = unsafe { (GetAsyncKeyState(0x11) as u16 & 0x8000) != 0 };
            let alt = unsafe { (GetAsyncKeyState(0x12) as u16 & 0x8000) != 0 };
            let shift = unsafe { (GetAsyncKeyState(0x10) as u16 & 0x8000) != 0 };

            if rec_slot >= 0 && rec_slot <= 2 {
                // --- RECORDING GAMEPAD (Slots 0..=2) ---
                if let Some(st) = state {
                    let b = st.gamepad.w_buttons;
                    let lt = st.gamepad.b_left_trigger > 120;
                    let rt = st.gamepad.b_right_trigger > 120;
                    if (b != 0 || lt || rt) && (!prev_lt && !prev_rt && prev_buttons == 0) {
                        let label = format_controller(b, lt, rt);
                        let mut g = bindings_input.lock().unwrap();
                        g[rec_slot as usize] = BoundAction::Controller { buttons: b, lt, rt, label: label.clone() };
                        recording_input.store(-1, Ordering::Relaxed);

                        let app_w = app_weak_input.clone();
                        let l_clone = label.clone();
                        let s_clone = rec_slot;
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = app_w.upgrade() {
                                let l = SharedString::from(l_clone);
                                match s_clone {
                                    0 => app.set_bind_pad_play_pause(l),
                                    1 => app.set_bind_pad_next(l),
                                    2 => app.set_bind_pad_prev(l),
                                    _ => {}
                                }
                                app.set_recording_slot(-1);
                            }
                        });
                    }
                    prev_buttons = b;
                    prev_lt = lt;
                    prev_rt = rt;
                }
            } else if rec_slot >= 3 && rec_slot <= 5 {
                // --- RECORDING KEYBOARD (Slots 3..=5) ---
                for vk in 8..=222 {
                    if vk == 0x10 || vk == 0x11 || vk == 0x12 || vk == 0x5B || vk == 0x5C {
                        continue;
                    }
                    let is_down = unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
                    if is_down && !prev_keys_down[vk as usize] {
                        let label = format_key(vk, ctrl, alt, shift);
                        let mut g = bindings_input.lock().unwrap();
                        g[rec_slot as usize] = BoundAction::Keyboard { vk, ctrl, alt, shift, label: label.clone() };
                        recording_input.store(-1, Ordering::Relaxed);

                        let app_w = app_weak_input.clone();
                        let l_clone = label.clone();
                        let s_clone = rec_slot;
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(app) = app_w.upgrade() {
                                let l = SharedString::from(l_clone);
                                match s_clone {
                                    3 => app.set_bind_key_play_pause(l),
                                    4 => app.set_bind_key_next(l),
                                    5 => app.set_bind_key_prev(l),
                                    _ => {}
                                }
                                app.set_recording_slot(-1);
                            }
                        });
                        break;
                    }
                    prev_keys_down[vk as usize] = is_down;
                }
            } else {
                // --- NORMAL DETECTION MODE ---
                let mut triggered_action = None;

                // Controller detection (Slots 0..=2)
                if pad_on {
                    if let Some(st) = state {
                        let b = st.gamepad.w_buttons;
                        let lt = st.gamepad.b_left_trigger > 120;
                        let rt = st.gamepad.b_right_trigger > 120;
                        let just_pressed = (b != 0 && prev_buttons == 0) || (lt && !prev_lt) || (rt && !prev_rt);

                        if just_pressed {
                            let g = bindings_input.lock().unwrap();
                            for slot in 0..=2 {
                                if let Some(bind) = g.get(slot) {
                                    let matched = match bind {
                                        BoundAction::Controller { buttons, lt: blt, rt: brt, .. } => {
                                            let bm = if *buttons != 0 { (b & *buttons) == *buttons } else { true };
                                            let lm = if *blt { lt } else { true };
                                            let rm = if *brt { rt } else { true };
                                            bm && lm && rm && (*buttons != 0 || *blt || *brt)
                                        }
                                        _ => false,
                                    };
                                    if matched {
                                        triggered_action = Some(slot % 3);
                                        break;
                                    }
                                }
                            }
                        }
                        prev_buttons = b;
                        prev_lt = lt;
                        prev_rt = rt;
                    }
                }

                // Keyboard detection (Slots 3..=5)
                if triggered_action.is_none() && kbd_on {
                    let g = bindings_input.lock().unwrap();
                    for slot in 3..=5 {
                        if let Some(bind) = g.get(slot) {
                            let matched = match bind {
                                BoundAction::Keyboard { vk, ctrl: c, alt: a, shift: s, .. } => {
                                    if ctrl == *c && alt == *a && shift == *s {
                                        let down = unsafe { (GetAsyncKeyState(*vk) as u16 & 0x8000) != 0 };
                                        let just_down = down && !prev_keys_down[*vk as usize];
                                        prev_keys_down[*vk as usize] = down;
                                        just_down
                                    } else {
                                        false
                                    }
                                }
                                _ => false,
                            };
                            if matched {
                                triggered_action = Some(slot % 3);
                                break;
                            }
                        }
                    }
                }

                // Execute action (0: Play/Pause, 1: Next, 2: Prev)
                if let Some(action_idx) = triggered_action {
                    let app_w = app_weak_input.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = app_w.upgrade() {
                            match action_idx {
                                0 => app.invoke_play_pause_clicked(),
                                1 => app.invoke_next_clicked(),
                                2 => app.invoke_prev_clicked(),
                                _ => {}
                            }
                        }
                    });
                }
            }

            std::thread::sleep(Duration::from_millis(50));
        }
    });

    let active_session = Arc::new(Mutex::new(
        None::<GlobalSystemMediaTransportControlsSession>,
    ));
    let latest_state = Arc::new(Mutex::new(None::<MediaState>));

    // Background Media Poller Thread (Runs at 700ms - low CPU)
    let state_store = latest_state.clone();
    let thread_session = active_session.clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .ok()
            .and_then(|op| op.get().ok());

        let mut known_title = String::new();
        let mut known_artist = String::new();
        let mut known_is_playing = false;
        let mut thumb_version = 0u64;

        loop {
            if manager.is_none() {
                manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                    .ok()
                    .and_then(|op| op.get().ok());
            }

            if let Some((state, session)) =
                fetch_media_state(&manager, &known_title, &known_artist, &mut thumb_version)
            {
                let changed = state.title != known_title
                    || state.artist != known_artist
                    || state.is_playing != known_is_playing
                    || state.thumbnail_bytes.is_some();

                if changed {
                    known_title = state.title.clone();
                    known_artist = state.artist.clone();
                    known_is_playing = state.is_playing;
                    *thread_session.lock().unwrap() = session;
                    *state_store.lock().unwrap() = Some(state);
                }
            } else if !known_title.is_empty() {
                known_title.clear();
                known_artist.clear();
                known_is_playing = false;
                *thread_session.lock().unwrap() = None;
                *state_store.lock().unwrap() = None;
                trim_working_set();
            }
            std::thread::sleep(Duration::from_millis(850));
        }
    });

    let cached_title_len = Arc::new(AtomicUsize::new(0));
    let title_len_media = cached_title_len.clone();
    let title_len_anim = cached_title_len.clone();

    // UI Updater Timer (Runs on Slint event loop)
    let state_consumer = latest_state.clone();
    let media_timer = Timer::default();
    let app_weak_for_media = app_weak.clone();
    let mut last_title = String::new();
    let mut last_is_playing = false;
    let mut last_thumb_version: u64 = 0;

    media_timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
        if let Some(app) = app_weak_for_media.upgrade() {
            let mut guard = state_consumer.lock().unwrap();
            if let Some(ref mut state) = *guard {
                let title_changed = last_title != state.title;

                if title_changed {
                    last_title = state.title.clone();
                    title_len_media.store(state.title.chars().count(), Ordering::Relaxed);
                    app.set_track_title(SharedString::from(state.title.clone()));
                    app.set_track_artist(SharedString::from(state.artist.clone()));
                }

                if state.thumbnail_version != last_thumb_version {
                    last_thumb_version = state.thumbnail_version;
                    if let Some(bytes) = state.thumbnail_bytes.take() {
                        if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                            let mut resized = dyn_img
                                .resize_exact(106, 106, image::imageops::FilterType::Triangle)
                                .to_rgba8();

                            // Mask image pixels strictly into an anti-aliased circle (106x106 1:1 screen resolution)
                            let cx = 53.0f32;
                            let cy = 53.0f32;
                            let radius = 52.5f32;
                            let r_inner = radius - 1.0;

                            for (x, y, pixel) in resized.enumerate_pixels_mut() {
                                let dx = x as f32 + 0.5 - cx;
                                let dy = y as f32 + 0.5 - cy;
                                let dist = (dx * dx + dy * dy).sqrt();
                                if dist > radius {
                                    pixel[3] = 0; // Transparent
                                } else if dist > r_inner {
                                    let factor = (radius - dist) / 1.0;
                                    pixel[3] = ((pixel[3] as f32) * factor) as u8;
                                }
                            }

                            let raw_data = resized.into_raw();
                            let mut pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::new(106, 106);
                            pixel_buffer.make_mut_bytes().copy_from_slice(&raw_data);
                            app.set_album_art(Image::from_rgba8(pixel_buffer));
                            app.set_has_album_art(true);
                        } else {
                            app.set_has_album_art(false);
                        }
                        // Immediately trim working set after decoding album art
                        trim_working_set();
                    } else if !app.get_has_album_art() {
                        app.set_has_album_art(false);
                    }
                }

                if last_is_playing != state.is_playing {
                    last_is_playing = state.is_playing;
                    app.set_is_playing(state.is_playing);
                    trim_working_set();
                }
            } else {
                if last_title != "NO MEDIA DETECTED" && !last_title.is_empty() {
                    last_title = "NO MEDIA DETECTED".to_string();
                    last_is_playing = false;
                    last_thumb_version = 0;
                    title_len_media.store(0, Ordering::Relaxed);
                    app.set_track_title(SharedString::from("NO MEDIA DETECTED"));
                    app.set_track_artist(SharedString::from(""));
                    app.set_is_playing(false);
                    app.set_has_album_art(false);
                    trim_working_set();
                }
            }
        }
    });

    // Vinyl Rotation Animation & Song Name Marquee Timer (Zero heap allocations, 0% CPU when idle)
    let app_weak_for_anim = app_weak.clone();
    let mut marquee_pause: u32 = 0;
    let mut current_speed: f32 = 0.0;
    let mut last_reported_angle: f32 = 0.0;
    let mut last_reported_offset: f32 = 0.0;
    let anim_timer = Timer::default();

    anim_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        if let Some(app) = app_weak_for_anim.upgrade() {
            let is_playing = app.get_is_playing();
            let rotation_enabled = app.get_rotation_enabled();
            let title_len = title_len_anim.load(Ordering::Relaxed);

            // Fast exit when completely idle: no music playing, vinyl at 0 deg, no marquee needed
            if !is_playing
                && current_speed <= 0.001
                && last_reported_angle == 0.0
                && title_len <= 15
                && last_reported_offset == 0.0
            {
                return;
            }

            // Vinyl spin with realistic acceleration & deceleration momentum
            if rotation_enabled {
                let target_speed = if is_playing { 1.6 } else { 0.0 };
                if current_speed < target_speed {
                    current_speed = (current_speed + 0.08).min(target_speed);
                } else if current_speed > target_speed {
                    current_speed = (current_speed - 0.05).max(0.0);
                }

                if current_speed > 0.001 {
                    let mut angle = last_reported_angle + current_speed;
                    if angle >= 360.0 {
                        angle -= 360.0;
                    }
                    last_reported_angle = angle;
                    app.set_rotation_angle(angle);
                }
            } else {
                // Vinyl Spin is disabled: smoothly return to straight 0-degree upright position!
                current_speed = 0.0;
                if last_reported_angle != 0.0 {
                    let diff = if last_reported_angle > 180.0 {
                        360.0 - last_reported_angle
                    } else {
                        -last_reported_angle
                    };
                    if diff.abs() < 1.0 {
                        last_reported_angle = 0.0;
                        app.set_rotation_angle(0.0);
                    } else {
                        let mut next = last_reported_angle + diff * 0.25;
                        if next >= 360.0 {
                            next -= 360.0;
                        }
                        if next < 0.0 {
                            next += 360.0;
                        }
                        last_reported_angle = next;
                        app.set_rotation_angle(next);
                    }
                }
            }

            // Marquee scroll for long titles (zero allocations: reads atomic title_len)
            if title_len > 15 {
                if marquee_pause > 0 {
                    marquee_pause -= 1;
                } else {
                    let mut cur = last_reported_offset + 0.8;
                    let max_scroll = (title_len as f32) * 7.5;
                    if cur > max_scroll {
                        cur = 0.0;
                        marquee_pause = 30;
                    }
                    last_reported_offset = cur;
                    app.set_scroll_offset(cur);
                }
            } else if last_reported_offset != 0.0 {
                last_reported_offset = 0.0;
                app.set_scroll_offset(0.0);
            }
        }
    });

    // Aggressive Periodic Memory Trimmer (Keeps working set locked strictly under 2.5MB)
    let mem_trim_timer = Timer::default();
    mem_trim_timer.start(TimerMode::Repeated, Duration::from_millis(800), || {
        trim_working_set();
    });

    // --- Controls ---
    let session_ctrl = active_session.clone();
    let app_weak_pp = app_weak.clone();
    app.on_play_pause_clicked(move || {
        if let Some(app) = app_weak_pp.upgrade() {
            let p = app.get_is_playing();
            app.set_is_playing(!p);
        }
        let s = session_ctrl.clone();
        std::thread::spawn(move || {
            let mut handled = false;
            if let Some(session) = s.lock().unwrap().as_ref() {
                if let Ok(op) = session.TryTogglePlayPauseAsync() {
                    if let Ok(res) = op.get() {
                        handled = res;
                    }
                }
            }
            if !handled {
                send_media_key(VK_MEDIA_PLAY_PAUSE);
            }
        });
    });

    let session_prev = active_session.clone();
    app.on_prev_clicked(move || {
        let s = session_prev.clone();
        std::thread::spawn(move || {
            let mut handled = false;
            if let Some(session) = s.lock().unwrap().as_ref() {
                if let Ok(op) = session.TrySkipPreviousAsync() {
                    if let Ok(res) = op.get() {
                        handled = res;
                    }
                }
            }
            if !handled {
                send_media_key(VK_MEDIA_PREV_TRACK);
            }
        });
    });

    let session_next = active_session.clone();
    app.on_next_clicked(move || {
        let s = session_next.clone();
        std::thread::spawn(move || {
            let mut handled = false;
            if let Some(session) = s.lock().unwrap().as_ref() {
                if let Ok(op) = session.TrySkipNextAsync() {
                    if let Ok(res) = op.get() {
                        handled = res;
                    }
                }
            }
            if !handled {
                send_media_key(VK_MEDIA_NEXT_TRACK);
            }
        });
    });

    // Volume Scroll with Visual Feedback Toast
    let app_weak_vol = app_weak.clone();
    let vol_timer_slot = Arc::new(Mutex::new(None::<Timer>));
    let vts = vol_timer_slot.clone();
    app.on_volume_changed(move |delta| {
        adjust_volume(delta > 0.0);
        if let Some(app) = app_weak_vol.upgrade() {
            app.set_volume_hud_text(SharedString::from(if delta > 0.0 {
                "VOL ▲"
            } else {
                "VOL ▼"
            }));
            app.set_show_volume_hud(true);
            let app_w = app.as_weak();
            let timer = Timer::default();
            timer.start(
                TimerMode::SingleShot,
                Duration::from_millis(850),
                move || {
                    if let Some(a) = app_w.upgrade() {
                        a.set_show_volume_hud(false);
                    }
                },
            );
            *vts.lock().unwrap() = Some(timer);
        }
    });

    let app_weak_close = app_weak.clone();
    app.on_close_app(move || {
        use windows::core::w;
        unsafe {
            if let Ok(h) = FindWindowW(None, w!("IAMMUSIC")) {
                if !h.0.is_null() {
                    let mut nid = NOTIFYICONDATAW::default();
                    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                    nid.hWnd = h;
                    nid.uID = 1001;
                    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                }
            }
        }
        if let Some(app) = app_weak_close.upgrade() {
            let _ = app.hide();
        }
        std::process::exit(0);
    });

    let app_weak_settings = app_weak.clone();
    app.on_toggle_settings(move || {
        if let Some(app) = app_weak_settings.upgrade() {
            let win = app.window();
            let pos = win.position();
            let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };

            let open_to_left = (pos.x + 350) > screen_width;
            let currently_open = app.get_settings_open();

            if !currently_open && open_to_left {
                win.set_position(slint::PhysicalPosition::new(pos.x - 210, pos.y));
            } else if currently_open && app.get_open_to_left() {
                win.set_position(slint::PhysicalPosition::new(pos.x + 210, pos.y));
            }

            app.set_open_to_left(open_to_left);
            app.set_settings_open(!currently_open);
        }
    });

    // Native Window Dragging using GetCursorPos delta
    let drag_start_cursor: Arc<Mutex<Option<(i32, i32)>>> = Arc::new(Mutex::new(None));
    let drag_start_win: Arc<Mutex<Option<(i32, i32)>>> = Arc::new(Mutex::new(None));

    let d_cursor_start = drag_start_cursor.clone();
    let d_win_start = drag_start_win.clone();
    let app_weak_drag_start = app_weak.clone();
    app.on_window_drag_start(move || {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        if let Some(app) = app_weak_drag_start.upgrade() {
            let pos = app.window().position();
            *d_cursor_start.lock().unwrap() = Some((pt.x, pt.y));
            *d_win_start.lock().unwrap() = Some((pos.x, pos.y));
        }
    });

    let d_cursor_move = drag_start_cursor.clone();
    let d_win_move = drag_start_win.clone();
    let app_weak_drag_move = app_weak.clone();
    app.on_window_drag_move(move || {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        let start_c = *d_cursor_move.lock().unwrap();
        let start_w = *d_win_move.lock().unwrap();
        if let (Some((sc_x, sc_y)), Some((sw_x, sw_y))) = (start_c, start_w) {
            let dx = pt.x - sc_x;
            let dy = pt.y - sc_y;
            if dx != 0 || dy != 0 {
                if let Some(app) = app_weak_drag_move.upgrade() {
                    app.window()
                        .set_position(slint::PhysicalPosition::new(sw_x + dx, sw_y + dy));
                }
            }
        }
    });

    let d_cursor_end = drag_start_cursor.clone();
    let d_win_end = drag_start_win.clone();
    app.on_window_drag_end(move || {
        *d_cursor_end.lock().unwrap() = None;
        *d_win_end.lock().unwrap() = None;
    });

    // --- Theme & Customization Callbacks ---
    let app_weak_bg = app_weak.clone();
    app.on_select_bg_preset(move |idx| {
        if let Some(app) = app_weak_bg.upgrade() {
            let color = match idx {
                1 => Color::from_rgb_u8(10, 10, 10),
                2 => Color::from_rgb_u8(43, 45, 66),
                3 => Color::from_rgb_u8(20, 27, 45),
                _ => Color::from_rgb_u8(30, 28, 25),
            };
            app.set_hud_bg_color(color);
        }
    });

    let app_weak_accent = app_weak.clone();
    app.on_select_accent_preset(move |idx| {
        if let Some(app) = app_weak_accent.upgrade() {
            let color = match idx {
                1 => Color::from_rgb_u8(255, 106, 61),
                2 => Color::from_rgb_u8(157, 78, 221),
                3 => Color::from_rgb_u8(0, 245, 212),
                4 => Color::from_rgb_u8(230, 57, 70),
                5 => Color::from_rgb_u8(252, 163, 17),
                _ => Color::from_rgb_u8(184, 77, 38),
            };
            app.set_hud_accent_color(color);
        }
    });

    let app_weak_op = app_weak.clone();
    app.on_change_opacity(move |val| {
        if let Some(app) = app_weak_op.upgrade() {
            app.set_hud_opacity(val);
        }
    });

    let app_weak_top = app_weak.clone();
    app.on_toggle_on_top(move || {
        if let Some(app) = app_weak_top.upgrade() {
            let top = app.get_is_always_on_top();
            app.set_is_always_on_top(!top);
        }
    });

    let app_weak_rot = app_weak.clone();
    app.on_toggle_rotation(move || {
        if let Some(app) = app_weak_rot.upgrade() {
            let rot = app.get_rotation_enabled();
            app.set_rotation_enabled(!rot);
            if rot {
                app.set_rotation_angle(0.0);
            }
        }
    });

    // Custom Color Picking & Hex Input Handling
    let app_weak_pick_accent = app_weak.clone();
    app.on_pick_custom_accent(move || {
        use windows::core::w;
        let hwnd = unsafe { FindWindowW(None, w!("IAMMUSIC")).unwrap_or_default() };
        if let Some(app) = app_weak_pick_accent.upgrade() {
            let current = app.get_hud_accent_color();
            if let Some(c) = open_native_color_picker(hwnd, current) {
                app.set_hud_accent_color(c);
                update_custom_slot(&app, c);
            }
        }
    });

    let app_weak_pick_bg = app_weak.clone();
    app.on_pick_custom_bg(move || {
        use windows::core::w;
        let hwnd = unsafe { FindWindowW(None, w!("IAMMUSIC")).unwrap_or_default() };
        if let Some(app) = app_weak_pick_bg.upgrade() {
            let current = app.get_hud_bg_color();
            if let Some(c) = open_native_color_picker(hwnd, current) {
                app.set_hud_bg_color(c);
                update_custom_slot(&app, c);
            }
        }
    });

    let app_weak_hex = app_weak.clone();
    app.on_apply_hex_color(move |hex_str, is_accent| {
        if let Some(app) = app_weak_hex.upgrade() {
            if let Some(c) = parse_hex_color(&hex_str) {
                if is_accent {
                    app.set_hud_accent_color(c);
                } else {
                    app.set_hud_bg_color(c);
                }
                update_custom_slot(&app, c);
            }
        }
    });

    let app_weak_cust = app_weak.clone();
    app.on_select_custom_color(move |c, is_accent| {
        if let Some(app) = app_weak_cust.upgrade() {
            if is_accent {
                app.set_hud_accent_color(c);
            } else {
                app.set_hud_bg_color(c);
            }
        }
    });

    // Window setup timer: Set frameless style, toolwindow, window icons, and System Tray icon
    let style_timer = Timer::default();
    style_timer.start(
        TimerMode::SingleShot,
        Duration::from_millis(50),
        move || {
            use windows::core::w;
            unsafe {
                let hwnd = FindWindowW(None, w!("IAMMUSIC"));
                if let Ok(h) = hwnd {
                    if !h.0.is_null() {
                        // 1. Install subclass procedure to completely suppress WM_NCACTIVATE / WM_NCPAINT / WM_NCCALCSIZE
                        let _ = SetWindowSubclass(h, Some(frameless_subclass_proc), 1001, 0);

                        // 2. Strip all caption and frame styles, enforce pure WS_POPUP
                        let mut style = GetWindowLongW(h, GWL_STYLE) as u32;
                        style &= !(0x00C00000 | 0x00040000 | 0x00020000 | 0x00010000 | 0x00080000); // WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU
                        style |= 0x80000000; // WS_POPUP
                        SetWindowLongW(h, GWL_STYLE, style as i32);

                        // 3. Set WS_EX_TOOLWINDOW on extended styles
                        let mut ex_style = GetWindowLongW(h, GWL_EXSTYLE) as u32;
                        ex_style |= WS_EX_TOOLWINDOW.0;
                        SetWindowLongW(h, GWL_EXSTYLE, ex_style as i32);

                        // 4. Clear the OS window title text to prevent DWM from displaying "IAMMUSIC" title header
                        let _ = SetWindowTextW(h, w!(""));

                        // 5. Trigger SWP_FRAMECHANGED so the 0-border WM_NCCALCSIZE takes effect immediately
                        let _ = SetWindowPos(
                            h,
                            HWND(std::ptr::null_mut()),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                        );

                        // 6. Explicitly disable DWM non-client rendering policy (DWMNCRP_DISABLED)
                        #[link(name = "dwmapi")]
                        extern "system" {
                            fn DwmSetWindowAttribute(
                                hwnd: HWND,
                                dw_attribute: u32,
                                pv_attribute: *const std::ffi::c_void,
                                cb_attribute: u32,
                            ) -> windows::core::HRESULT;
                        }
                        let nc_policy: u32 = 1; // DWMNCRP_DISABLED
                        let _ = DwmSetWindowAttribute(h, 2, &nc_policy as *const _ as *const std::ffi::c_void, 4);

                        // Load icon and set Window + Taskbar icons
                        let icon_str: Vec<u16> = "icon.ico\0".encode_utf16().collect();
                        let hicon_res = LoadImageW(
                            None,
                            windows::core::PCWSTR(icon_str.as_ptr()),
                            IMAGE_ICON,
                            0,
                            0,
                            LR_LOADFROMFILE | LR_DEFAULTSIZE,
                        );
                        if let Ok(hicon) = hicon_res {
                            if !hicon.0.is_null() {
                                let _ = SendMessageW(
                                    h,
                                    WM_SETICON,
                                    WPARAM(ICON_BIG as usize),
                                    LPARAM(hicon.0 as isize),
                                );
                                let _ = SendMessageW(
                                    h,
                                    WM_SETICON,
                                    WPARAM(ICON_SMALL as usize),
                                    LPARAM(hicon.0 as isize),
                                );

                                // System Tray Notification Icon
                                let mut nid = NOTIFYICONDATAW::default();
                                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                                nid.hWnd = h;
                                nid.uID = 1001;
                                nid.uFlags = NIF_ICON | NIF_TIP;
                                nid.hIcon = HICON(hicon.0);
                                let tip = "IAMMUSIC\0".encode_utf16().collect::<Vec<u16>>();
                                for (i, &c) in tip.iter().enumerate().take(nid.szTip.len()) {
                                    nid.szTip[i] = c;
                                }
                                let _ = Shell_NotifyIconW(NIM_ADD, &nid);
                            }
                        }

                        // Initial memory trim
                        trim_working_set();
                    }
                }
            }
        },
    );

    app.run()?;
    Ok(())
}
