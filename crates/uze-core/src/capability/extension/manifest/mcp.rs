//! `mcp "rtc" plugin="std@cardinal" timeout="5m"`: whose MCP server the
//! extension's `mcp:` grants call, and how long a call may take
//! (`extension-mcp` D2, D7).

use kdl::KdlNode;
use uze_contract::{MCP_TIMEOUT_MAX, McpDecl, is_mcp_name};

use super::{prop, string_arg, string_prop};
use crate::capability::extension::document::{Place, Report, entries, refuse_extra};

pub(super) fn decode(node: &KdlNode, report: &mut Report<'_>) -> Option<McpDecl> {
    let parts = entries(node, report);
    refuse_extra(node, &parts, 1, &["plugin", "timeout"], false, report);
    let server = string_arg(node, parts.args.first().copied(), report)?;
    if !is_mcp_name(&server) {
        report.on(
            node,
            format!("`{server}` is not an MCP server's name"),
            Some("write the name its plugin's `mcp.json` gives it".to_owned()),
        );
        return None;
    }
    let plugin = prop(&parts, "plugin").and_then(|entry| {
        let written = string_prop("plugin", entry, report)?;
        if is_plugin_reference(&written) {
            Some(written)
        } else {
            report.at(
                entry.offset(),
                format!("`{written}` is not a plugin as `<plugin>@<marketplace>`"),
                Some("write it as `uze` lists it, such as `std@cardinal`".to_owned()),
            );
            None
        }
    });
    let timeout = prop(&parts, "timeout").and_then(|entry| {
        let written = string_prop("timeout", entry, report)?;
        match super::source::duration(&written) {
            Some(timeout) if !timeout.is_zero() && timeout <= MCP_TIMEOUT_MAX => Some(timeout),
            _ => {
                report.at(
                    entry.offset(),
                    format!("`{written}` is not a call's time"),
                    Some(format!(
                        "write a duration up to {} minutes, such as `timeout=\"5m\"`",
                        MCP_TIMEOUT_MAX.as_secs() / 60
                    )),
                );
                None
            }
        }
    });
    Some(McpDecl {
        server,
        plugin,
        timeout,
    })
}

/// `<plugin>@<marketplace>`, each a package's name: lowercase letters,
/// digits, `-`, `_` and `.`.
fn is_plugin_reference(written: &str) -> bool {
    let name = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.')
            })
    };
    written
        .split_once('@')
        .is_some_and(|(plugin, marketplace)| name(plugin) && name(marketplace))
}
