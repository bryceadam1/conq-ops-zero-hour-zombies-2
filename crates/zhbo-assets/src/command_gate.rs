//! The first-person commands' coverage gate over the player's own install (docs/COMMANDS.md
//! section 12): every `CommandButton` in Zero Hour's INIs uses a command type the language can
//! press (`zhbo_core::command::actions::BUTTONS`: a verb or a grammar form, never one marked absent
//! or not an order).
//!
//! The INIs name a command type as `TheGuiCommandNames` spells it (`ControlBar.h`): the enum name
//! without `GUI_COMMAND_` or `GUICOMMANDMODE_`.

#[cfg(test)]
mod tests {
    use zhbo_core::command::actions::{Covered, BUTTONS};

    fn ini_name(engine: &str) -> &str {
        engine.strip_prefix("GUI_COMMAND_").or_else(|| engine.strip_prefix("GUICOMMANDMODE_")).unwrap_or(engine)
    }

    /// The `CommandButton` blocks of an INI text: each button's name and its `Command`.
    fn buttons(text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let code = line.split(';').next().unwrap_or("").trim();
            if let Some(name) = code.strip_prefix("CommandButton ") {
                current = Some(name.trim().to_string());
            } else if code.eq_ignore_ascii_case("End") {
                current = None;
            } else if let (Some(name), Some(rest)) = (&current, code.strip_prefix("Command")) {
                if let Some(value) = rest.trim_start().strip_prefix('=') {
                    out.push((name.clone(), value.trim().to_string()));
                }
            }
        }
        out
    }

    #[test]
    fn the_command_button_parser_reads_blocks() {
        let text = "CommandButton Command_Stop\n  Command = STOP ; comment\n  TextLabel = X\nEnd\nCommandButton Command_Sell\n  Command       = SELL\nEnd\n";
        assert_eq!(buttons(text), [("Command_Stop".to_string(), "STOP".to_string()), ("Command_Sell".to_string(), "SELL".to_string())]);
    }

    #[test]
    #[ignore = "reads this machine's Zero Hour install (ZHBO_ZH_DIR)"]
    fn every_installed_command_button_can_be_pressed_by_a_command() {
        let zh = zhbo_requirements::check_requirements().zero_hour.expect("a Zero Hour install").path;
        let mut found = Vec::new();
        for archive in ["INIZH.big", "PatchINI.big"] {
            let Ok(big) = zhbo_formats::big::BigArchive::open(zh.join(archive)) else { continue };
            for entry in big.entries() {
                if entry.path.to_ascii_lowercase().ends_with(".ini") {
                    let text = String::from_utf8_lossy(&big.read(entry).unwrap()).into_owned();
                    found.extend(buttons(&text).into_iter().map(|(n, c)| (archive, n, c)));
                }
            }
        }
        assert!(found.len() > 500, "only {} command buttons found", found.len());
        let mut uncovered = Vec::new();
        for (archive, name, command) in &found {
            match BUTTONS.iter().find(|(engine, _, _)| ini_name(engine) == command) {
                Some((_, _, Covered::Verb(_) | Covered::Grammar(_))) => {}
                Some((engine, _, how)) => uncovered.push(format!("{archive} {name}: {command} ({engine}: {how:?})")),
                None => uncovered.push(format!("{archive} {name}: {command} is no engine command type")),
            }
        }
        eprintln!("{} command buttons, every one pressable", found.len());
        assert!(uncovered.is_empty(), "{uncovered:#?}");
    }
}
