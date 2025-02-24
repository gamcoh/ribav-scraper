use std::collections::HashMap;
use tracing::{info, warn};

use docx_rust::document::{BreakType, Run, TextSpace};
use docx_rust::formatting::{CharacterProperty, CharacterStyleId, Color, Size, UnderlineStyle};
use scraper::Node;
use scraper::{CaseSensitivity, ElementRef};

pub trait CharacterPropertyExt {
    fn merge(&self, other: &Self) -> Self;
}

impl CharacterPropertyExt for CharacterProperty<'_> {
    fn merge(&self, other: &Self) -> Self {
        Self {
            bold: self.bold.to_owned().or(other.bold.to_owned()),
            italics: self.italics.to_owned().or(other.italics.to_owned()),
            underline: self.underline.to_owned().or(other.underline.to_owned()),
            size: self.size.to_owned().or(other.size.to_owned()),
            color: self.color.to_owned().or(other.color.to_owned()),
            style_id: self.style_id.to_owned().or(other.style_id.to_owned()),
            ..Default::default()
        }
    }
}

pub fn parse_recursive<'a>(container: ElementRef, last_element_is_citation: bool) -> Vec<Run<'a>> {
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
                paragraphs.extend(parse_html_to_docx_format(el, last_element_is_citation));
            }
            _ => {
                info!("Unknown node: {:?}", node);
            }
        }
    }

    paragraphs
}

pub fn parse_html_to_docx_format<'a>(
    el: Option<ElementRef>,
    last_element_is_citation: bool,
) -> Vec<Run<'a>> {
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
                let children = parse_recursive(el.child_elements().last().unwrap(), true);
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

            let mut cp = CharacterProperty::default();

            if *properties.get("font-weight").unwrap_or(&"") == "bold" {
                cp = cp.bold(true);
            }

            if *properties.get("font-style").unwrap_or(&"") == "italics" {
                cp = cp.italics(true);
            }

            if *properties.get("text-decoration").unwrap_or(&"") == "underline" {
                cp = cp.underline(UnderlineStyle::Single);
            }

            if properties.get("font-size").is_some() {
                let size = properties
                    .get("font-size")
                    .unwrap()
                    .trim_end_matches("px")
                    .parse::<u8>()
                    .unwrap();
                cp = cp.size(Size::from(size.lt(&15).then_some(16u8).unwrap_or(size)));
            }

            if properties.get("color").is_some() {
                let color = (*properties.get("color").unwrap()).to_string();

                match color.as_ref() {
                    "blue" => {
                        cp = cp.color(Color::from((0, 0, 255)));
                    }
                    _ => {
                        warn!("Unknown color: {}", color);
                    }
                }
            }

            if !last_element_is_citation {
                paragraphs.push(Run::default().push_text((" ", TextSpace::Preserve)));
            }

            for child in parse_recursive(el, false) {
                let mut cp = cp.clone();
                if let Some(ref child_cp) = child.property {
                    cp = cp.merge(child_cp);
                }
                paragraphs.push(child.property(cp));
            }
            paragraphs.push(Run::default().push_text((" ", TextSpace::Preserve)));
        }
        _ => {
            panic!("Unknown tag: {}", el.value().name());
        }
    }

    paragraphs
}
