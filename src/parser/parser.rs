use std::collections::HashMap;

use docx_rust::document::{BreakType, Run, TextSpace};
use docx_rust::formatting::CharacterProperty;
use scraper::{CaseSensitivity, ElementRef};
use tracing::info;

pub fn parse_html_to_docx_format<'a>(el: Option<ElementRef>) -> Vec<Run<'a>> {
    let mut paragraphs = Vec::new();

    if el.is_none() {
        return paragraphs;
    }

    let el = el.unwrap();

    match el.value().name() {
        "div" => {
            if el
                .value()
                .has_class("postrow-message", CaseSensitivity::CaseSensitive)
            {
                return paragraphs;
            }

            if el
                .value()
                .has_class("border-blue-500", CaseSensitivity::CaseSensitive)
            {
                paragraphs.push(
                    Run::default()
                        .property(CharacterProperty::default().bold(true))
                        .push_text("Citation: ")
                        .push_break(BreakType::TextWrapping),
                );
                // Skip the "Citation: " part
                let t = el.text().skip(10).collect::<String>();
                paragraphs.push(Run::default().push_text(t.trim_start().to_owned()));
            } else {
                unimplemented!(
                    "Unknown div class: {:?} with text {:?}",
                    el.value(),
                    el.text().collect::<String>()
                );
            }
        }
        "br" => {
            paragraphs.push(
                Run::default()
                    .push_text("")
                    .push_break(BreakType::TextWrapping),
            );
        }
        "span" => {
            let properties = el
                .attr("style")
                .unwrap_or_default()
                .split(';')
                .map(|prop| {
                    let mut split = prop.split(':');
                    let key = split.next().unwrap();
                    let value = split.next().unwrap();
                    (key, value)
                })
                .collect::<HashMap<_, _>>();

            let text = el.text().collect::<String>();

            paragraphs.push(
                Run::default()
                    .property(
                        CharacterProperty::default()
                            .bold(*properties.get("font-weight").unwrap_or(&"") == "bold")
                            .italics(*properties.get("font-style").unwrap_or(&"") == "italic"),
                    )
                    .push_text((" ", TextSpace::Preserve))
                    .push_text(text)
                    .push_text((" ", TextSpace::Preserve)),
            );
        }
        _ => {
            info!("Unknown tag: {}", el.value().name());
        }
    }

    paragraphs
}
