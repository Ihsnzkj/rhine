//! GDK exposes human-readable XKB names; map them to canonical short layout codes.
use std::{collections::HashMap, sync::OnceLock};
pub fn short_name(label: &str) -> String {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            let mut names = HashMap::new();
            if let Ok(xml) = std::fs::read_to_string("/usr/share/X11/xkb/rules/evdev.xml")
                && let Ok(doc) = roxmltree::Document::parse_with_options(
                    &xml,
                    roxmltree::ParsingOptions {
                        allow_dtd: true,
                        ..Default::default()
                    },
                )
            {
                for layout in doc.descendants().filter(|n| n.has_tag_name("layout")) {
                    let config = layout.children().find(|n| n.has_tag_name("configItem"));
                    let code = config
                        .and_then(|n| n.children().find(|n| n.has_tag_name("name")))
                        .and_then(|n| n.text());
                    if let Some(code) = code {
                        for desc in layout
                            .descendants()
                            .filter(|n| n.has_tag_name("description"))
                        {
                            if let Some(text) = desc.text() {
                                names.insert(text.to_string(), code.to_string());
                            }
                        }
                    }
                }
            }
            names
        })
        .get(label)
        .cloned()
        .unwrap_or_default()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_xkb_name() {
        assert_eq!(short_name("English (US)"), "us");
        assert_eq!(short_name(""), "");
        assert_eq!(short_name("custom-layout"), "");
    }
}
