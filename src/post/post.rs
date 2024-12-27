use crate::extract;
use crate::parser::parser::parse_html_to_docx_format;
use crate::utils::functions::anonymize_author;
use anyhow::Result;
use docx_rust::document::{BreakType, Paragraph, Run};
use docx_rust::formatting::{
    CharacterProperty, JustificationVal, ParagraphProperty, UnderlineStyle,
};
use docx_rust::Docx;
use scraper::{CaseSensitivity, ElementRef, Node};
use scraper::{Html, Selector};

use tracing::info;

#[derive(Debug, Default, Clone)]
pub struct Post {
    pub title: String,
    pub html: Option<Html>,
    pub messages: Option<Vec<PostMessage>>,
    pub last_author: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PostMessage {
    pub author: String,
    pub date: String,
    pub message: String,
}

impl Into<Vec<Run<'_>>> for PostMessage {
    fn into(self) -> Vec<Run<'static>> {
        let html = Html::parse_fragment(&self.message);
        let container = html
            .select(&Selector::parse(".postrow-message").unwrap())
            .next()
            .unwrap();

        let mut paragraphs = Vec::new();
        let mut children = container.descendants();

        let mut index = 0;
        while let Some(node) = children.next() {
            index += 1;
            match node.value() {
                Node::Text(text) => {
                    let text = text.text.trim();
                    paragraphs.push(Run::default().push_text(text.to_owned()));
                }
                Node::Element(ref _elem) => {
                    let el = ElementRef::wrap(node);
                    paragraphs.extend(parse_html_to_docx_format(el));
                    if el.unwrap().value().name().ne("br") && index > 1 {
                        children.next();
                    }

                    if el
                        .unwrap()
                        .value()
                        .has_class("border-blue-500", CaseSensitivity::CaseSensitive)
                        && index > 1
                    {
                        let needs_to_skip = el.unwrap().children().collect::<Vec<_>>().len();
                        children.nth(needs_to_skip);
                        continue;
                    }
                }
                _ => {
                    info!("Unknown node: {:?}", node);
                }
            }
        }

        paragraphs
    }
}

impl Post {
    pub fn save(&mut self) -> Result<()> {
        self._get_messages()?;
        self._messages_to_word()?;

        Ok(())
    }

    fn _messages_to_word(&mut self) -> Result<()> {
        let mut docx = Docx::default();
        docx.document.push(
            Paragraph::default()
                .push(
                    Run::default()
                        .push_text(self.title.to_owned())
                        .property(CharacterProperty::default().bold(true).size(32 as u8))
                        .push_break(BreakType::TextWrapping)
                        .push_break(BreakType::TextWrapping),
                )
                .property(ParagraphProperty::default().justification(JustificationVal::Center)),
        );

        for message in self.messages.as_ref().unwrap() {
            let author_p = if message.author.contains("Binyamin Wattenberg") {
                if self.last_author.is_some()
                    && self
                        .last_author
                        .as_ref()
                        .unwrap()
                        .contains("Binyamin Wattenberg")
                {
                    Paragraph::default().push(Run::default().push_text(""))
                } else {
                    Paragraph::default().push(
                        Run::default()
                            .push_break(BreakType::TextWrapping)
                            .push_text("Réponse:")
                            .property(
                                CharacterProperty::default()
                                    .bold(true)
                                    .size(24 as u8)
                                    .underline(UnderlineStyle::Single),
                            )
                            .push_break(BreakType::TextWrapping),
                    )
                }
            } else {
                Paragraph::default().push(
                    Run::default()
                        .push_break(BreakType::TextWrapping)
                        .push_text(format!(
                            "Question par {}:",
                            anonymize_author(message.author.to_owned())
                        ))
                        .property(
                            CharacterProperty::default()
                                .bold(true)
                                .size(24 as u8)
                                .underline(UnderlineStyle::Single),
                        )
                        .push_break(BreakType::TextWrapping),
                )
            };

            self.last_author = Some(message.author.clone());

            let message_p: Vec<Run> = (*message).clone().into();

            docx.document.push(author_p);
            let mut pa = Paragraph::default();

            for run in message_p {
                pa = pa.push(run)
            }

            docx.document.push(pa);
            docx.document
                .push(Paragraph::default().push(Run::default().push_text("")));
        }

        docx.write_file(format!(
            "files_generated/{}.docx",
            self.title
                .escape_default()
                .collect::<String>()
                .replace("/", "_")
        ))
        .unwrap();

        Ok(())
    }

    fn _get_messages(&mut self) -> Result<()> {
        let html = self
            .html
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HTML not fetched for post"))?;

        let posts_sel =
            Selector::parse(".container > .overflow-hidden.border-blue-500 > div > .flex")
                .map_err(|e| anyhow::anyhow!("Failed to parse posts selector: {}", e))?;
        let author_sel = Selector::parse("div strong.block.mb-2")
            .map_err(|e| anyhow::anyhow!("Failed to parse author selector: {}", e))?;
        let date_sel = Selector::parse("a.text-blue-link")
            .map_err(|e| anyhow::anyhow!("Failed to parse date selector: {}", e))?;
        let message_sel = Selector::parse(".py-4.postrow-message")
            .map_err(|e| anyhow::anyhow!("Failed to parse message selector: {}", e))?;

        html.select(&posts_sel).for_each(|post| {
            let author = extract!(post, &author_sel);
            let date = extract!(post, &date_sel);
            let message = extract!(post, &message_sel, html);

            // We need to update the messages field of the post
            let post_message = PostMessage {
                author,
                date,
                message,
            };
            self.messages
                .get_or_insert_with(Vec::new)
                .push(post_message);
        });

        Ok(())
    }
}
