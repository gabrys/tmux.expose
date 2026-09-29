use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::model::Session;

// tmux escapes control characters embedded in a format string. The unit
// separator below therefore appears in command output as the four printable
// characters `\037`, which is still a safe delimiter for user-defined names.
const FIELD_SEPARATOR: &str = "\\037";
const PANE_FORMAT: &str = "#{pane_id}\u{1f}#{window_id}\u{1f}#{E:window-status-format}\u{1f}#{window_name}\u{1f}#{window_active}\u{1f}#{pane_active}\u{1f}#{window_bell_flag}\u{1f}#{window_panes}\u{1f}#{pane_index}";

pub fn list_panes() -> Result<Vec<Session>> {
    list_panes_skipping_preview_for(None)
}

pub fn list_panes_skipping_preview_for(current_pane_id: Option<&str>) -> Result<Vec<Session>> {
    let output = Command::new("tmux")
        .args(["list-panes", "-s", "-F", PANE_FORMAT])
        .output()
        .context("failed to run tmux list-panes")?;

    if !output.status.success() {
        return Err(tmux_error("tmux list-panes failed", &output.stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut panes = parse_panes_output(&stdout)?;

    for pane in &mut panes {
        if !should_capture_preview(&pane.id, current_pane_id) {
            pane.preview.clear();
            pane.preview_error = Some("Current pane preview disabled".to_string());
            continue;
        }

        match capture_pane_preview(&pane.id, 200) {
            Ok(preview) => {
                pane.preview = preview;
                pane.preview_error = None;
            }
            Err(error) => {
                pane.preview.clear();
                pane.preview_error = Some(error.to_string());
            }
        }
    }

    Ok(panes)
}

pub fn current_pane_id() -> Result<Option<String>> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{pane_id}"])
        .output()
        .context("failed to run tmux display-message")?;

    if !output.status.success() {
        return Ok(None);
    }

    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!id.is_empty()).then_some(id))
}

pub fn capture_pane_preview(pane_target: &str, max_lines: usize) -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-e", "-p", "-t", pane_target])
        .output()
        .with_context(|| format!("failed to capture tmux pane '{pane_target}'"))?;

    if !output.status.success() {
        return Err(tmux_error("tmux capture-pane failed", &output.stderr));
    }

    Ok(trim_preview(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        max_lines,
    ))
}

pub fn select_pane(window_target: &str, pane_target: &str) -> Result<()> {
    let window_status = Command::new("tmux")
        .args(["select-window", "-t", window_target])
        .status()
        .with_context(|| format!("failed to select tmux window '{window_target}'"))?;

    if !window_status.success() {
        return Err(anyhow!("tmux select-window failed for '{window_target}'"));
    }

    let pane_status = Command::new("tmux")
        .args(["select-pane", "-t", pane_target])
        .status()
        .with_context(|| format!("failed to select tmux pane '{pane_target}'"))?;

    if !pane_status.success() {
        return Err(anyhow!("tmux select-pane failed for '{pane_target}'"));
    }

    Ok(())
}

pub fn parse_panes(output: &str) -> Vec<Session> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(9, FIELD_SEPARATOR);
            let id = parts.next()?.to_string();
            let window_id = parts.next()?.to_string();
            let formatted_name = parts.next()?.trim();
            let fallback_name = parts.next()?.trim();
            let base_name = if formatted_name.is_empty() {
                fallback_name
            } else {
                formatted_name
            };
            if base_name.is_empty() {
                return None;
            }

            let window_active = parts.next().is_some_and(|value| value != "0");
            let pane_active = parts.next().is_some_and(|value| value != "0");
            let attached = window_active && pane_active;
            let bell = parts.next().is_some_and(|value| value != "0");
            let pane_count = parts
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1);
            let pane_index = parts.next().unwrap_or("?");
            let name = if pane_count > 1 {
                format!("{base_name} · pane {pane_index}")
            } else {
                base_name.to_string()
            };

            Some(Session {
                id,
                window_id,
                name,
                attached,
                bell,
                preview: Vec::new(),
                preview_error: None,
            })
        })
        .collect()
}

fn parse_panes_output(output: &str) -> Result<Vec<Session>> {
    let panes = parse_panes(output);
    if panes.is_empty() && !output.trim().is_empty() {
        return Err(anyhow!(
            "tmux list-panes returned output in an unexpected format"
        ));
    }

    Ok(panes)
}

pub fn trim_preview(output: &str, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = output
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();

    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    let start = lines.len().saturating_sub(max_lines);
    lines.into_iter().skip(start).collect()
}

fn tmux_error(message: &str, stderr: &[u8]) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        anyhow!(message.to_string())
    } else {
        anyhow!("{message}: {stderr}")
    }
}

fn should_capture_preview(pane_id: &str, current_pane_id: Option<&str>) -> bool {
    current_pane_id != Some(pane_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pane_lines_from_tmux_format() {
        let panes = parse_panes(
            "%1\\037@1\\037 [1: ~/src/app: vim] \\037editor\\0371\\0371\\0371\\0371\\0370\n%2\\037@2\\037 [2: ~/var/log: tail] \\037logs\\0370\\0371\\0370\\0371\\0370\n",
        );

        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0].id, "%1");
        assert_eq!(panes[0].window_id, "@1");
        assert_eq!(panes[0].name, "[1: ~/src/app: vim]");
        assert!(panes[0].attached);
        assert!(panes[0].bell);
        assert_eq!(panes[1].name, "[2: ~/var/log: tail]");
        assert!(!panes[1].attached);
        assert!(!panes[1].bell);
    }

    #[test]
    fn parses_pane_names_containing_colons() {
        let panes = parse_panes(
            "%3\\037@3\\037dev:api\\037fallback\\0370\\0371\\0370\\0371\\0370\n",
        );

        assert_eq!(panes[0].id, "%3");
        assert_eq!(panes[0].name, "dev:api");
    }

    #[test]
    fn falls_back_to_window_name_when_status_format_is_empty() {
        let panes = parse_panes(
            "%4\\037@4\\037   \\037shell\\0370\\0371\\0370\\0371\\0370\n",
        );

        assert_eq!(panes[0].name, "shell");
    }

    #[test]
    fn split_window_panes_get_distinguishing_suffixes() {
        let panes = parse_panes(
            "%4\\037@4\\037shell\\037fallback\\0371\\0371\\0370\\0372\\0370\n%5\\037@4\\037shell\\037fallback\\0371\\0370\\0370\\0372\\0371\n",
        );

        assert_eq!(panes[0].name, "shell · pane 0");
        assert_eq!(panes[1].name, "shell · pane 1");
        assert!(panes[0].attached);
        assert!(!panes[1].attached);
    }

    #[test]
    fn active_pane_in_an_inactive_window_is_not_attached() {
        let panes = parse_panes(
            "%4\\037@4\\037shell\\037fallback\\0370\\0371\\0370\\0371\\0370\n",
        );

        assert!(!panes[0].attached);
    }

    #[test]
    fn rejects_non_empty_unparseable_pane_output() {
        let error = parse_panes_output("%3dev01171000300\n")
            .unwrap_err()
            .to_string();

        assert!(error.contains("unexpected format"));
    }

    #[test]
    fn trims_preview_to_last_non_empty_visible_lines() {
        let preview = trim_preview("first\nsecond\nthird\n\n", 2);

        assert_eq!(preview, vec!["second".to_string(), "third".to_string()]);
    }

    #[test]
    fn preserves_ansi_escape_sequences_from_preview() {
        let preview = trim_preview("\u{1b}[31mred\u{1b}[0m plain", 5);

        assert_eq!(preview, vec!["\u{1b}[31mred\u{1b}[0m plain".to_string()]);
    }

    #[test]
    fn skips_preview_capture_for_current_pane_only_when_requested() {
        assert!(!should_capture_preview("%1", Some("%1")));
        assert!(should_capture_preview("%2", Some("%1")));
        assert!(should_capture_preview("%1", None));
    }
}
