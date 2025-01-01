use crate::extract;
use crate::http::client::get_html;
use crate::parser::parser::parse_recursive;
use crate::utils::constants::BASE_URL;
use crate::utils::functions::{anonymize_author, is_citation};
use anyhow::Result;
use docx_rust::document::{BreakType, Paragraph, Run};
use docx_rust::formatting::{
    CharacterProperty, Indent, JustificationVal, ParagraphProperty, UnderlineStyle,
};
use docx_rust::{Docx, DocxFile};
use reqwest::Client;
use scraper::{Html, Selector};

#[derive(Debug, Default, Clone)]
pub struct Post {
    pub title: String,
    pub html: Option<Html>,
    pub messages: Option<Vec<PostMessage>>,
    pub last_author: Option<String>,
    pub category: String,
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

        parse_recursive(container)
    }
}

impl Post {
    pub async fn save(&mut self, client: &Client) -> Result<()> {
        self._get_messages(&client).await?;
        self._messages_to_word()?;

        Ok(())
    }

    fn _messages_to_word(&mut self) -> Result<()> {
        let docx_file = DocxFile::from_file(format!(
            "files_generated/{}.docx",
            self.category
                .escape_default()
                .collect::<String>()
                .replace("/", "_")
        ));

        let file;
        let mut docx = if docx_file.is_ok() {
            file = docx_file.unwrap();
            file.parse().unwrap()
        } else {
            Docx::default()
        };

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

            let message_p: Vec<Run> = message.to_owned().into();

            docx.document.push(author_p);

            // Adding the date
            docx.document.push(
                Paragraph::default().push(
                    Run::default()
                        .push_text(format!("Le {}", message.date.replace("Posté le: ", "")))
                        .property(
                            CharacterProperty::default()
                                .bold(true)
                                .underline(UnderlineStyle::Single),
                        )
                        .push_break(BreakType::TextWrapping),
                ),
            );

            let mut messages_iter = message_p.into_iter();
            while let Some(run) = messages_iter.next() {
                // Is this run a citation?
                if is_citation(&run) {
                    let mut p = Paragraph::default().property(ParagraphProperty::default().indent(
                        Indent {
                            left: Some(300),
                            ..Default::default()
                        },
                    ));
                    p = p.push(run);
                    let mut last_run = None;
                    while let Some(next_run) = messages_iter.next() {
                        if !is_citation(&next_run) {
                            last_run = Some(next_run);
                            break;
                        }
                        p = p.push(next_run);
                    }
                    docx.document.push(p);
                    if let Some(last_run) = last_run {
                        docx.document.push(Paragraph::default().push(last_run));
                    }
                } else {
                    let mut p = Paragraph::default();
                    p = p.push(run);
                    let mut last_run = None;
                    while let Some(next_run) = messages_iter.next() {
                        if is_citation(&next_run) {
                            last_run = Some(next_run);
                            break;
                        }
                        p = p.push(next_run);
                    }
                    docx.document.push(p);

                    if let Some(last_run) = last_run {
                        docx.document
                            .push(Paragraph::default().push(last_run).property(
                                ParagraphProperty::default().indent(Indent {
                                    left: Some(300),
                                    ..Default::default()
                                }),
                            ));
                    }
                }
            }

            docx.document
                .push(Paragraph::default().push(Run::default().push_text("")));
        }

        docx.write_file(format!(
            "files_generated/{}.docx",
            self.category
                .escape_default()
                .collect::<String>()
                .replace("/", "_")
        ))
        .unwrap();

        Ok(())
    }

    async fn _get_messages(&mut self, client: &Client) -> Result<()> {
        let mut html = self
            .html
            .clone()
            .ok_or_else(|| anyhow::anyhow!("HTML not fetched for post"))?;

        loop {
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

            // If there are other pages, we need to replace the HTML field with the next page
            let next_page_sel = Selector::parse("nav.pagination > a[href^='suivante']")
                .map_err(|e| anyhow::anyhow!("Failed to parse next page selector: {}", e))?;

            let next_page = html.select(&next_page_sel).next();
            if next_page.is_none() {
                break;
            }

            let url = next_page.unwrap().value().attr("href").unwrap();
            let url = format!("{}{}", BASE_URL, url);
            html = get_html(client, url).await?.0;
        }

        Ok(())
    }
}
