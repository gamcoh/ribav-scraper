use std::collections::HashMap;
use tracing::info;

use docx_rust::document::{BreakType, Run, TextSpace};
use docx_rust::formatting::{CharacterProperty, CharacterStyleId, UnderlineStyle};
use scraper::Node;
use scraper::{CaseSensitivity, ElementRef};

pub fn parse_recursive<'a>(container: ElementRef) -> Vec<Run<'a>> {
    let mut paragraphs = Vec::new();
    let mut children = container.children();

    while let Some(node) = children.next() {
        match node.value() {
            Node::Text(text) => {
                let text = text.text.trim();
                paragraphs.push(Run::default().push_text(text.to_owned()));
            }
            Node::Element(ref _elem) => {
                let el = ElementRef::wrap(node);
                paragraphs.extend(parse_html_to_docx_format(el));
            }
            _ => {
                info!("Unknown node: {:?}", node);
            }
        }
    }

    paragraphs
}

pub fn parse_html_to_docx_format<'a>(el: Option<ElementRef>) -> Vec<Run<'a>> {
    let mut paragraphs = Vec::new();

    if el.is_none() {
        return paragraphs;
    }

    let el = el.unwrap();

    match el.value().name() {
        "a" => {
            paragraphs.push(
                Run::default()
                    .property(CharacterProperty::default().underline(UnderlineStyle::Single))
                    .push_text(el.text().collect::<String>()),
            );
        }
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
                        .property(
                            CharacterProperty::default()
                                .bold(true)
                                .style_id(CharacterStyleId::from("citation")),
                        )
                        .push_text("Citation: "),
                );

                // last div on the citation block
                let children = parse_recursive(el.child_elements().last().unwrap());
                let children = children.into_iter().map(|c| {
                    c.property(
                        CharacterProperty::default().style_id(CharacterStyleId::from("citation")),
                    )
                });
                paragraphs.extend(children);
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

            let cp = CharacterProperty::default()
                .bold(*properties.get("font-weight").unwrap_or(&"") == "bold")
                .italics(*properties.get("font-style").unwrap_or(&"") == "italic")
                .size(
                    properties
                        .get("font-size")
                        .unwrap_or(&"18px")
                        .trim_end_matches("px")
                        .parse::<u8>()
                        .unwrap(),
                );

            paragraphs.push(Run::default().push_text((" ", TextSpace::Preserve)));
            for child in parse_recursive(el) {
                paragraphs.push(child.property(cp.clone()));
            }
            paragraphs.push(Run::default().push_text((" ", TextSpace::Preserve)));
        }
        _ => {
            panic!("Unknown tag: {}", el.value().name());
        }
    }

    paragraphs
}
