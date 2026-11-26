//! A pen on a Wayland desktop opens the window through Xwayland (#816).
//!
//! Wayland compositors give a pen only to clients that bind the tablet protocol (`zwp_tablet_v2`),
//! which winit 0.30 doesn't (and binding it on winit's connection needs `unsafe`, which this app
//! forbids). Some compositors (KWin) then send the window nothing from the pen: no cursor, no
//! painting. Xwayland binds the protocol and serves the pen as an X11 pointer, so with a pen
//! attached the window opens there. `TEXTURECRAFT_NATIVE_WAYLAND=1` keeps native Wayland.
//!
//! Adapted from PhotoCraft (storytold/photocraft `830316b7`, MIT OR Apache-2.0):
//! `apps/photocraft/src/tablet.rs` (`display_session`, `pen_needs_xwayland`, `has_pen`, `bit_set`)
//! and `apps/photocraft/src/linux_libs.rs` (`session_from_env`, `ldconfig_has`, the library
//! search), reduced to the pen decision. Replace it with a shared craft-libs crate once one exists
//! (AGENTS.md rule 9).

use std::path::{Path, PathBuf};

/// Set (and not `0`) to keep native Wayland with a pen attached
const NATIVE_WAYLAND_VARIABLE: &str = "TEXTURECRAFT_NATIVE_WAYLAND";

/// `BTN_TOOL_PEN` (linux/input-event-codes.h): the key a tablet's stylus, a pen display's or a
/// pen-enabled touchscreen's pen reports
const BTN_TOOL_PEN: u32 = 0x140;

/// `ABS_X` and `ABS_Y` (linux/input-event-codes.h): the absolute axes every pen reports
const ABS_X: u32 = 0x00;
const ABS_Y: u32 = 0x01;

/// What winit 0.30 loads at run time for an X11 window: Xlib, Xlib-xcb, xcb, Xcursor, XInput2 and
/// xkbcommon with its X11 part
const X11_LIBRARIES: &[&str] =
    &["libX11.so.6", "libX11-xcb.so.1", "libxcb.so.1", "libXcursor.so.1", "libXi.so.6", "libxkbcommon.so.0", "libxkbcommon-x11.so.0"];

/// Whether the window opens through Xwayland: a pen is attached on a Wayland desktop and Xwayland
/// can run the window (the X11 libraries are installed and the X server answers). Says so on
/// stderr when it does.
pub fn opens_through_xwayland() -> bool {
    let var = |name: &str| std::env::var(name).ok();
    let pen = || std::fs::read_to_string("/proc/bus/input/devices").is_ok_and(|devices| has_pen(&devices));
    let xwayland_ready = || x11_libraries_available() && var("DISPLAY").is_some_and(|display| x_server_answers(&display));
    if !pen_needs_xwayland(var, pen, xwayland_ready) {
        return false;
    }

    eprintln!("texturecraft: opening the window through Xwayland for the pen ({NATIVE_WAYLAND_VARIABLE}=1 keeps Wayland)");
    true
}

/// [`opens_through_xwayland`]'s decision. winit picks Wayland when `WAYLAND_DISPLAY` or
/// `WAYLAND_SOCKET` is set; Xwayland needs `DISPLAY`. The system probes run only on Wayland.
fn pen_needs_xwayland(var: impl Fn(&str) -> Option<String>, pen: impl FnOnce() -> bool, xwayland_ready: impl FnOnce() -> bool) -> bool {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    let wayland = set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET");
    let keeps_wayland = var(NATIVE_WAYLAND_VARIABLE).is_some_and(|value| !value.is_empty() && value != "0");
    wayland && !keeps_wayland && set("DISPLAY") && pen() && xwayland_ready()
}

/// Does the X server `display` names accept a connection? winit can't fall back to Wayland once
/// X11 is chosen, so a `DISPLAY` left over from another session must keep the window on Wayland.
/// Only a local display is tried, by its socket path or the abstract socket of the same name.
fn x_server_answers(display: &str) -> bool {
    use std::os::linux::net::SocketAddrExt as _;
    use std::os::unix::net::{SocketAddr, UnixStream};

    let Some(socket) = local_x_socket(display) else { return false };
    UnixStream::connect(&socket).is_ok() || SocketAddr::from_abstract_name(socket.as_bytes()).and_then(|address| UnixStream::connect_addr(&address)).is_ok()
}

/// The socket of a local X display (`:0`, `:0.0`, `unix:0`); `None` for a remote one
fn local_x_socket(display: &str) -> Option<String> {
    let number = display.strip_prefix(':').or_else(|| display.strip_prefix("unix:"))?;
    let number: u32 = number.split('.').next()?.parse().ok()?;
    Some(format!("/tmp/.X11-unix/X{number}"))
}

/// Is a pen among the input devices `/proc/bus/input/devices` lists? A pen's `B: KEY=` bitmap has
/// `BTN_TOOL_PEN` and its `B: ABS=` bitmap has `ABS_X` and `ABS_Y`, as libinput asks of a tablet
/// tool. The key alone isn't enough: a virtual keyboard made with uinput claims every key code.
fn has_pen(devices: &str) -> bool {
    devices.split("\n\n").any(|device| {
        let bitmap = |name: &str| device.lines().find_map(|line| line.strip_prefix(name));
        bitmap("B: KEY=").is_some_and(|keys| bit_set(keys, BTN_TOOL_PEN)) && bitmap("B: ABS=").is_some_and(|axes| bit_set(axes, ABS_X) && bit_set(axes, ABS_Y))
    })
}

/// Bit `bit` of a `/proc/bus/input/devices` bitmap: hex words of the kernel's `long`, most
/// significant first. Short or malformed bitmaps have no bits set.
fn bit_set(bitmap: &str, bit: u32) -> bool {
    let word_bits = usize::BITS;
    let word = usize::try_from(bit / word_bits).unwrap_or(usize::MAX);
    bitmap.split_whitespace().rev().nth(word).and_then(|hex| u64::from_str_radix(hex, 16).ok()).is_some_and(|value| (value >> (bit % word_bits)) & 1 == 1)
}

/// Are the libraries an X11 window needs installed? The linker cache (`ldconfig -p`) or the usual
/// library directories must list each; one that can't be found counts as missing.
fn x11_libraries_available() -> bool {
    let cache = linker_cache();
    let dirs = library_dirs(std::env::var("LD_LIBRARY_PATH").ok().as_deref());
    X11_LIBRARIES.iter().all(|soname| cache.as_deref().is_some_and(|cache| ldconfig_has(cache, soname)) || dirs.iter().any(|dir| dir.join(soname).exists()))
}

/// `ldconfig -p` in English (it is parsed), from where distributions put it
fn linker_cache() -> Option<String> {
    ["/sbin/ldconfig", "/usr/sbin/ldconfig", "ldconfig"].into_iter().find_map(|command| {
        let output = std::process::Command::new(command).arg("-p").env("LC_ALL", "C").output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    })
}

/// Does an `ldconfig -p` listing have `soname` for this build's architecture? Lines look like
/// `\tlibXi.so.6 (libc6,x86-64) => /lib/x86_64-linux-gnu/libXi.so.6`.
fn ldconfig_has(cache: &str, soname: &str) -> bool {
    let architecture = if cfg!(target_arch = "x86_64") {
        Some("x86-64")
    } else if cfg!(target_arch = "aarch64") {
        Some("AArch64")
    } else {
        None
    };

    cache.lines().any(|line| {
        let Some((name, rest)) = line.trim_start().split_once(' ') else { return false };
        let flags = rest.split_once('(').and_then(|(_, after)| after.split_once(')')).map_or("", |(flags, _)| flags);
        name == soname && architecture.is_none_or(|tag| flags.split(',').any(|flag| flag.trim() == tag))
    })
}

/// `LD_LIBRARY_PATH`, then the usual system and Flatpak library directories
fn library_dirs(ld_library_path: Option<&str>) -> Vec<PathBuf> {
    let triplet = if cfg!(target_arch = "aarch64") { "aarch64-linux-gnu" } else { "x86_64-linux-gnu" };
    let mut dirs: Vec<PathBuf> = ld_library_path.into_iter().flat_map(|value| value.split(':')).filter(|dir| !dir.is_empty()).map(PathBuf::from).collect();
    dirs.extend(["/app/lib", "/usr/local/lib", "/usr/lib64", "/lib64", "/usr/lib", "/lib"].map(PathBuf::from));
    dirs.extend([Path::new("/usr/lib").join(triplet), Path::new("/lib").join(triplet)]);
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A touchpad (`BTN_TOOL_FINGER`, no pen) and a Wacom pen, from `/proc/bus/input/devices`
    const TOUCHPAD: &str = "N: Name=\"SYNA2B33:00 06CB:CE26 Touchpad\"\nB: KEY=e520 10000 0 0 0 0\nB: ABS=2e0800000000003\n";
    const PEN: &str = "N: Name=\"Wacom Intuos Pro M Pen\"\nB: KEY=1c03 0 0 0 0 0\nB: ABS=1000f000003\n";

    /// A uinput keyboard (RustDesk's) claims every key, `BTN_TOOL_PEN` included, with no axes
    const VIRTUAL_KEYBOARD: &str = "N: Name=\"RustDesk UInput Keyboard\"\nB: EV=20013\nB: KEY=ffffffffff 0 7ffffff000301ff 1000f7fffffff 7fe001fffff000f 7ffffffffffffff ffffffff0003fdff 7fff8fff00ff03ff 1ffffffffffff07 ffffffffffffffff ffffffffffefffff fffffffffffffffe\n";

    #[test]
    fn a_pen_is_a_device_with_the_pen_key_and_absolute_axes() {
        assert!(has_pen(&format!("{TOUCHPAD}\n{PEN}")));
        assert!(!has_pen(TOUCHPAD), "a touchpad is no pen");
        assert!(!has_pen(&format!("{VIRTUAL_KEYBOARD}\n{TOUCHPAD}")), "a keyboard claiming the pen key has no pen axes");
        assert!(!has_pen("B: KEY=zz 0 0 0 0 0\nB: ABS=3\n"), "a malformed bitmap is no pen");
    }

    #[test]
    fn a_pen_on_wayland_opens_the_window_through_xwayland() {
        fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
            move |name: &str| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
        }
        const WAYLAND: &[(&str, &str)] = &[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")];

        assert!(pen_needs_xwayland(env(WAYLAND), || true, || true));
        assert!(!pen_needs_xwayland(env(WAYLAND), || false, || true), "no pen");
        assert!(!pen_needs_xwayland(env(WAYLAND), || true, || false), "Xwayland can't run the window");
        assert!(!pen_needs_xwayland(env(&[("WAYLAND_DISPLAY", "wayland-0")]), || true, || true), "no Xwayland");
        assert!(!pen_needs_xwayland(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0"), (NATIVE_WAYLAND_VARIABLE, "1")]), || true, || true), "kept");
        assert!(pen_needs_xwayland(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0"), (NATIVE_WAYLAND_VARIABLE, "0")]), || true, || true));
        assert!(!pen_needs_xwayland(env(&[("DISPLAY", ":0")]), || panic!("probed off Wayland"), || panic!("probed off Wayland")), "X11 already has the pen");
        assert_eq!([":1.0", "unix:0", "remote:0"].map(local_x_socket), [Some("/tmp/.X11-unix/X1".into()), Some("/tmp/.X11-unix/X0".into()), None]);
    }
}
