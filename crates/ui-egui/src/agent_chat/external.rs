//! External agents (Preferences › AI › External agents): the control channel that MCP clients
//! you run yourself (Hermes, OpenClaw, Claude Desktop, Cursor, Claude Code…) reach through
//! `solidcraft-cli mcp --connect` (docs/mcp.md). The setting starts the same loopback channel as
//! `solidcraft --control PORT`, token included, without a command-line flag; the page shows the
//! line each client needs to add SolidCraft.

use std::sync::mpsc::Receiver;

use crate::i18n::{format as trf, tr};
use crate::{ControlRequest, SolveApp};

/// Starts the control channel on a port: its requests, or why the port could not be used. The
/// desktop host sets it; without one (the browser) the setting has nothing to start.
pub type Starter = Box<dyn FnMut(u16) -> Result<Receiver<ControlRequest>, String>>;

/// The clients the page shows a configuration for, in the order of its segments.
pub const CLIENTS: [&str; 4] = ["JSON", "Hermes", "OpenClaw", "Claude Code"];

#[derive(Default)]
pub struct External {
    pub starter: Option<Starter>,
    /// The port the control channel listens on in this launch (`--control` or this setting).
    pub listening: Option<u16>,
    /// The port a start failed on and why; not tried again on that port until asked.
    pub failed: Option<(u16, String)>,
    /// The client whose configuration the page shows (index into [`CLIENTS`]).
    pub client: usize,
}

/// Start the channel when the setting is on and nothing listens yet (each frame; cheap).
pub fn tick(app: &mut SolveApp) {
    let port = app.preferences.mcp_port;
    let external = &mut app.agent_chat.external;
    if !app.preferences.mcp_enabled || external.listening.is_some() || external.failed.as_ref().is_some_and(|(p, _)| *p == port) {
        return;
    }
    let Some(start) = external.starter.as_mut() else { return };
    match start(port) {
        Ok(rx) => {
            external.listening = Some(port);
            external.failed = None;
            app.control_rx = Some(rx);
        }
        Err(error) => external.failed = Some((port, error)),
    }
}

/// The text a client needs to add SolidCraft: `cli` is `solidcraft-cli`, `port` the channel's.
pub fn snippet(client: usize, cli: &str, port: u16) -> String {
    let address = format!("127.0.0.1:{port}");
    match CLIENTS.get(client).copied().unwrap_or_default() {
        "Hermes" => format!("hermes mcp add solidcraft --command {} --args mcp --connect {address}", quoted(cli)),
        "OpenClaw" => format!("openclaw mcp add solidcraft --command {} --arg mcp --arg --connect --arg {address}", quoted(cli)),
        "Claude Code" => format!("claude mcp add solidcraft -- {} mcp --connect {address}", quoted(cli)),
        _ => {
            let config = serde_json::json!({"mcpServers": {"solidcraft": {"command": cli, "args": ["mcp", "--connect", address]}}});
            serde_json::to_string_pretty(&config).unwrap_or_default()
        }
    }
}

/// `path` as one shell word.
fn quoted(path: &str) -> String {
    if path.chars().all(|c| c.is_ascii_alphanumeric() || "/._-:\\".contains(c)) {
        path.to_string()
    } else {
        format!("\"{}\"", path.replace('"', "\\\""))
    }
}

/// The page's section. The address is one placeholder, so a right-to-left sentence keeps it whole.
#[cfg(not(target_arch = "wasm32"))]
pub fn section(app: &mut SolveApp, ui: &mut egui::Ui) {
    use egui::RichText;
    let t = crate::theme::Tokens::get();
    super::settings::section(ui, tr("ui.mcp_title"));
    let r = ui.checkbox(&mut app.preferences.mcp_enabled, tr("ui.mcp_enable"));
    crate::scenario::publish_handle("prefs:mcp_enabled", r.rect.center());
    super::settings::note(ui, tr("ui.mcp_note"));
    ui.horizontal(|ui| {
        ui.label(tr("ui.mcp_port"));
        ui.add_enabled_ui(app.agent_chat.external.listening.is_none(), |ui| {
            ui.add(egui::DragValue::new(&mut app.preferences.mcp_port).range(1024..=65535));
        });
    });
    let port = app.preferences.mcp_port;
    match (app.agent_chat.external.listening, app.agent_chat.external.failed.clone()) {
        (Some(listening), _) => {
            let text = trf("ui.mcp_listening", &[("address", &format!("127.0.0.1:{listening}"))]);
            ui.label(RichText::new(text).color(t.text_dim));
            if !app.preferences.mcp_enabled || listening != port {
                super::settings::note(ui, tr("ui.mcp_restart"));
            }
        }
        (None, Some((failed, error))) if app.preferences.mcp_enabled && failed == port => {
            let message = trf("ui.mcp_failed", &[("port", &failed.to_string()), ("error", &error)]);
            ui.add(egui::Label::new(RichText::new(message).color(t.error)).wrap());
            if ui.button(tr("ui.mcp_retry")).clicked() {
                app.agent_chat.external.failed = None;
            }
        }
        _ => {}
    }
    if !app.preferences.mcp_enabled && app.agent_chat.external.listening.is_none() {
        return;
    }
    let Some(cli) = super::find_cli() else {
        ui.add(egui::Label::new(RichText::new(tr("ui.mcp_no_cli")).color(t.error)).wrap());
        return;
    };
    let port = app.agent_chat.external.listening.unwrap_or(port);
    ui.horizontal_wrapped(|ui| {
        for (index, name) in CLIENTS.iter().enumerate() {
            ui.radio_value(&mut app.agent_chat.external.client, index, *name);
        }
    });
    let text = snippet(app.agent_chat.external.client, &cli.to_string_lossy(), port);
    ui.add(egui::Label::new(RichText::new(&text).monospace().size(11.0)).wrap());
    if ui.button(tr("ui.agent_chat_copy")).clicked() {
        ui.ctx().copy_text(text);
    }
    super::settings::note(ui, tr("ui.mcp_config_note"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_client_gets_the_connect_line_for_the_port() {
        let cli = "/Applications/SolidCraft.app/Contents/MacOS/solidcraft-cli";
        let json: serde_json::Value = serde_json::from_str(&snippet(0, cli, 7878)).unwrap();
        assert_eq!(json["mcpServers"]["solidcraft"]["command"], cli);
        assert_eq!(json["mcpServers"]["solidcraft"]["args"], serde_json::json!(["mcp", "--connect", "127.0.0.1:7878"]));
        assert_eq!(snippet(1, cli, 7900), format!("hermes mcp add solidcraft --command {cli} --args mcp --connect 127.0.0.1:7900"));
        assert_eq!(snippet(2, cli, 7878), format!("openclaw mcp add solidcraft --command {cli} --arg mcp --arg --connect --arg 127.0.0.1:7878"));
        assert_eq!(snippet(3, cli, 7878), format!("claude mcp add solidcraft -- {cli} mcp --connect 127.0.0.1:7878"));
        // A path with a space stays one word.
        assert!(
            snippet(1, "C:\\Program Files\\SolidCraft\\solidcraft-cli.exe", 7878)
                .contains("--command \"C:\\Program Files\\SolidCraft\\solidcraft-cli.exe\" ")
        );
    }

    /// The setting starts the channel once, on its port; a port that failed is not tried again
    /// each frame, and a new port is.
    #[test]
    fn the_setting_starts_the_channel_once_and_reports_a_busy_port() {
        let mut h = crate::scenario::Harness::new();
        let tried = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = tried.clone();
        h.app.agent_chat.external.starter = Some(Box::new(move |port| {
            seen.borrow_mut().push(port);
            if port == 7878 { Err("address in use".into()) } else { Ok(std::sync::mpsc::channel().1) }
        }));
        tick(&mut h.app);
        assert!(tried.borrow().is_empty(), "off by default");
        h.app.preferences.mcp_enabled = true;
        tick(&mut h.app);
        tick(&mut h.app);
        assert_eq!(*tried.borrow(), [7878], "a failed port is not retried every frame");
        assert_eq!(h.app.agent_chat.external.failed.as_ref().map(|(p, _)| *p), Some(7878));
        h.app.preferences.mcp_port = 7900;
        tick(&mut h.app);
        tick(&mut h.app);
        assert_eq!(*tried.borrow(), [7878, 7900]);
        assert_eq!(h.app.agent_chat.external.listening, Some(7900));
        assert!(h.app.control_rx.is_some());
    }

    /// A launch with `--control` already listens: the setting starts nothing more.
    #[test]
    fn a_control_flag_launch_counts_as_listening() {
        let mut h = crate::scenario::Harness::new();
        h.app.agent_chat.external.listening = Some(7878);
        h.app.agent_chat.external.starter = Some(Box::new(|_| Err("must not start".into())));
        h.app.preferences.mcp_enabled = true;
        tick(&mut h.app);
        assert!(h.app.agent_chat.external.failed.is_none());
    }
}
