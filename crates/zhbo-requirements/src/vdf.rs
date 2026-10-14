//! Minimal reader for Valve KeyValues text (`libraryfolders.vdf`, `appmanifest_*.acf`).

/// First `"key" "value"` pair for `key` (case-insensitive).
pub fn value(text: &str, key: &str) -> Option<String> {
    values(text, key).into_iter().next()
}

/// Every `"key" "value"` pair for `key` (case-insensitive), with `\\` unescaped.
pub fn values(text: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let quoted: Vec<&str> = line.split('"').collect();
        // `"key"\t\t"value"` splits into ["", key, "\t\t", value, ""].
        if quoted.len() >= 5 && quoted[1].eq_ignore_ascii_case(key) {
            out.push(quoted[3].replace("\\\\", "\\"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders_and_manifests() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/home/me/.local/share/Steam\"\n\t}\n}\n";
        assert_eq!(values(vdf, "path"), vec![r"C:\Program Files (x86)\Steam", "/home/me/.local/share/Steam"]);
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"202970\"\n\t\"installdir\"\t\t\"Call of Duty Black Ops II\"\n}\n";
        assert_eq!(value(acf, "installdir").as_deref(), Some("Call of Duty Black Ops II"));
    }
}
