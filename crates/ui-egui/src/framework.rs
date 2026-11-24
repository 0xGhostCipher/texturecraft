//! The craft-ui runtime, hosted from TextureCraft's own eframe `App` (craft-libs docs/craft-framework.md, section 2.2).
//! The desktop app calls [`install`] once, [`TextureCraftApp::before_input`] in `raw_input_hook`,
//! [`TextureCraftApp::close_path`] last in `logic`, and [`before_ui`] first in `ui`; the browser build and tests
//! install with [`install_headless`]. The framework dialogs run on [`TextureCraftApp::framework_dialogs`] after
//! TextureCraft's own UI.
//!
//! The runtime takes Escape, Tab and the clipboard events for the whole window unless one of TextureCraft's own egui
//! widgets has the keyboard. The window's Escape is read with [`escape_pressed`] and taken with
//! [`take_window_cancel`]; the 3D/2D viewport is declared to the runtime with [`host_canvas`], which keeps Tab (Hide
//! UI), the edit chords and the clipboard events in egui's input for TextureCraft's shortcuts.

use std::sync::{Arc, OnceLock};

use craft_host::target::ShellInfo;
use craft_host::{AppInfo, Host};
use craft_keymap::Overrides;
use craft_ui::fonts::{I18nScripts, SystemFonts, UiFonts};
use craft_ui::runtime::commands::{CommandInput, CommandSet as _, NavCommand, QuitCommand};
use craft_ui::runtime::env::{RootEnv, Scale};
use craft_ui::runtime::focus::{EditSet, KeepKeys, KeyUse};
use craft_ui::runtime::frame::{FrameInput, FrameOutput};
use craft_ui::runtime::keys::{Intent, with_window_intents};
use craft_ui::shell::embed::{self, CloseDecision};
use craft_window::policy::Decision;
use craft_window::system::WindowSystem;
use egui::ViewportId;

use crate::TextureCraftApp;

/// Each pass of the root window, first in `App::ui`: hands the runtime the native window it answers the hit test of
/// (Windows) and fits the window to its monitor once.
#[cfg(not(target_arch = "wasm32"))]
pub use craft_ui::shell::embed::before_ui;

/// The id every OS window of the app carries, as the desktop entry and the update app name it.
const APP_ID: &str = "ai.storyteller.texturecraft";

/// The scripts of the interface languages, for the font stack.
static SCRIPTS: I18nScripts = I18nScripts;

/// The runtime's first font stack: egui's bundled faces and no installed system fonts. TextureCraft's own definitions
/// ([`crate::theme::install`]) replace it before anything is drawn.
fn ui_fonts() -> UiFonts<'static> {
    UiFonts::new(&SCRIPTS).system_fonts(SystemFonts::Off)
}

fn install_with(ctx: &egui::Context, shell: ShellInfo, windows: Arc<dyn WindowSystem>, frame: Option<Decision>) -> Result<(), String> {
    let mut spec = craft_ui::InstallSpec::new(shell, windows).app_id(APP_ID);
    if let Some(frame) = frame {
        spec = spec.frame(frame);
    }
    embed::install(ctx, spec, &ui_fonts()).map(drop).map_err(|error| format!("the UI runtime could not be installed: {error}"))
}

/// Installs the runtime on the desktop window's context, once, before the first pass, with the window's `frame`
/// ([`crate::titlebar::TitleBar::decide`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn install(ctx: &egui::Context, frame: Decision) -> Result<(), String> {
    let shell = ShellInfo::default().backend(craft_host::target::Backend::from_env());
    install_with(ctx, shell, Arc::new(craft_window::system::SystemWindows), Some(frame))
}

/// Installs the runtime on a context with no native window of its own: the browser build's canvas and test windows.
pub fn install_headless(ctx: &egui::Context) {
    if let Err(error) = install_with(ctx, ShellInfo::default(), Arc::new(craft_window::system::NoWindows), None) {
        log::warn!("{error}");
    }
}

/// Who TextureCraft is to the framework's dialogs (the update dialogs show its name and icon).
pub(crate) fn host() -> Arc<Host> {
    Arc::new(
        Host::builder(AppInfo { id: "texturecraft", name: "TextureCraft", version: texturecraft_engine::VERSION })
            .icon_png(include_bytes!("../../../assets/app-icon/hicolor/128x128/apps/ai.storyteller.texturecraft.png"))
            .build(),
    )
}

/// TextureCraft's own shortcut table keeps the keys the framework binds by default: Ctrl+Q is `ui.quit`, F6 is
/// `camera.orthographic` (and Shift+F6 stays free for it), so the framework's quit and pane moves are unbound.
/// Overrides are keyed by command id, which the command sets give.
fn command_overrides() -> Arc<Overrides> {
    static OVERRIDES: OnceLock<Arc<Overrides>> = OnceLock::new();
    OVERRIDES
        .get_or_init(|| {
            let mut overrides = Overrides::new();
            for id in [QuitCommand::Quit.id(), NavCommand::NextPane.id(), NavCommand::PreviousPane.id()] {
                overrides.insert(id, Vec::new());
            }
            Arc::new(overrides)
        })
        .clone()
}

impl TextureCraftApp {
    /// The framework's part of `App::raw_input_hook`, once per frame before the first pass: takes what the runtime
    /// gathered since the last frame and gives the coming passes the root values (theme mode and interface scale).
    pub fn before_input(&mut self, ctx: &egui::Context) {
        // TextureCraft applies the interface scale itself (`logic`), so the runtime keeps the zoom it applied last.
        let scale = ctx.zoom_factor();
        embed::before_input(ctx, |output| self.frame_input(ctx, output, scale));
    }

    /// The root values of this frame. TextureCraft registers no commands with the runtime (its own shortcut table
    /// runs every key, [`command_overrides`]) and lets it toggle no chrome (Tab is Hide UI, kept in egui by the
    /// viewport's [`host_canvas`]), so the output carries nothing it acts on. While one of TextureCraft's own egui
    /// modals shows in the root window, the runtime blocks the panels floated into windows of their own.
    pub(crate) fn frame_input(&mut self, ctx: &egui::Context, _output: FrameOutput, scale: f32) -> FrameInput {
        use crate::theme::Brightness;
        let mode = match self.ui.prefs.brightness {
            Brightness::Light | Brightness::MediumLight => craft_theme::Mode::Light,
            Brightness::Dark | Brightness::MediumDark => craft_theme::Mode::Dark,
        };
        let root = RootEnv::new().mode(mode).scale(Scale::new(scale));
        let app_modal = crate::docking::input_blocked(self, ctx).then_some(ViewportId::ROOT);
        FrameInput::new(root).commands(CommandInput::new().overrides(command_overrides())).host(Arc::clone(&self.framework_host)).app_modal(app_modal)
    }

    /// The framework's part of `App::logic`, after TextureCraft's own: the root window's close path. Unsaved changes
    /// hold the window open while the Unsaved Changes dialog asks ([`crate::windows::close_requested`]); otherwise
    /// it closes.
    pub fn close_path(&mut self, ctx: &egui::Context) {
        embed::logic(ctx, || if crate::windows::close_requested(self) { CloseDecision::Hold } else { CloseDecision::Close });
    }
}

/// Declares the viewport `id`, drawn in this pass, as the window's document canvas. With nothing focused it is the
/// window's key target: Tab (Hide UI), the edit chords and the clipboard events stay in egui's input for TextureCraft's
/// shortcuts.
pub(crate) fn host_canvas(ctx: &egui::Context, id: egui::Id) {
    const KEYS: KeyUse = KeyUse::CANVAS.with_keep(KeepKeys::Tab).with_edits(EditSet::ALL);
    craft_ui::runtime::focus::host_canvas(ctx, id, KEYS);
}

/// Whether Escape was pressed in this pass: from egui while one of TextureCraft's own widgets or popups has the
/// keyboard, otherwise the window's cancel, which the runtime routes to no component. Leaves it for later readers.
pub(crate) fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.key_pressed(egui::Key::Escape)) || window_cancel(ctx, false)
}

/// Takes the window's cancel (an Escape while nothing has framework focus) when it is there: no later reader sees it.
pub(crate) fn take_window_cancel(ctx: &egui::Context) -> bool {
    window_cancel(ctx, true)
}

/// Whether the window's cancel is there and untaken; `take` takes it.
fn window_cancel(ctx: &egui::Context, take: bool) -> bool {
    let mut pending = false;
    with_window_intents(ctx, false, |intents| {
        pending = if take { intents.take(Intent::Cancel) } else { intents.untaken(&Intent::Cancel) };
        take && pending
    });
    pending
}
