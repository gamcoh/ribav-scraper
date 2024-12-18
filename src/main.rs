use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use docx_rust::document::{BreakType, Paragraph, Run, TextSpace};
use docx_rust::formatting::{CharacterProperty, ParagraphProperty, Spacing};
use docx_rust::Docx;
use encoding_rs::WINDOWS_1252;
use futures::future::join_all;
use reqwest::{header, Client};
use scraper::{selectable::Selectable, Html, Selector};
use scraper::{ElementRef, Node};
use std::collections::HashMap;
use tokio::{self};

use tracing::{info, warn, Level};

const MAX_PAGES: u16 = 1;
const PAGE_SIZE: u16 = 50;
const BASE_URL: &str = "https://www.techouvot.com/";

// extract!(post, &author_sel, text) -> post.select(&author_sel).next().unwrap().text().collect::<String>().trim().to_string()
macro_rules! extract {
    ($post:ident, $sel:expr) => {
        $post
            .select($sel)
            .next()
            .unwrap()
            .text()
            .collect::<String>()
            .trim()
            .to_string()
    };
    ($post:ident, $sel:expr, $want:ident) => {
        $post.select($sel).next().unwrap().$want()
    };
}

#[derive(Debug, Default, Clone)]
struct Post {
    title: String,
    html: Option<Html>,
    messages: Option<Vec<PostMessage>>,
}

#[derive(Debug, Clone)]
struct PostMessage {
    author: String,
    date: String,
    message: String,
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
                }
                _ => {
                    info!("Unknown node: {:?}", node);
                }
            }
        }

        paragraphs
    }
}

fn parse_html_to_docx_format<'a>(el: Option<ElementRef>) -> Vec<Run<'a>> {
    let mut paragraphs = Vec::new();

    if el.is_none() {
        return paragraphs;
    }

    match el.unwrap().value().name() {
        "div" => {
            info!("Div found");
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
                .unwrap()
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

            let text = el.unwrap().text().collect::<String>();

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
            info!("Unknown tag: {}", el.unwrap().value().name());
        }
    }

    paragraphs
}

impl Post {
    fn save(&mut self) -> Result<()> {
        self._get_messages()?;
        self._messages_to_word()?;

        Ok(())
    }

    fn _messages_to_word(&self) -> Result<()> {
        let mut docx = Docx::default();

        for message in self.messages.as_ref().unwrap() {
            let author = format!("{} ({})", message.author, message.date);

            let author_p = Paragraph::default().push_text(author);
            let message_p: Vec<Run> = (*message).clone().into();

            docx.document.push(author_p);
            let mut pa = Paragraph::default();

            for run in message_p {
                pa = pa.push(run)
            }

            docx.document.push(pa);
            docx.document.push(
                Paragraph::default().push(
                    Run::default()
                        .push_text("")
                        .push_break(BreakType::TextWrapping)
                        .push_break(BreakType::TextWrapping),
                ),
            );
        }

        docx.write_file(format!(
            "files_generated/{}.docx",
            self.title.escape_default()
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

fn number_days_since_2020() -> i64 {
    let today = Utc::now();
    let start_of_2020 = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    (today - start_of_2020).num_days()
}

#[tokio::main(flavor = "current_thread")] // Use current_thread runtime for blocking operations
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .init();

    // Build a reqwest client with a timeout to be more production-ready
    let client = Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    let url = "https://www.techouvot.com/search.php?mode=results";
    let mut posts = HashMap::new();

    let page = 0;
    let (doc, _) = get_html(&client, url)
        .await
        .context("Failed to get initial HTML page")?;

    let next_page_url = find_next_page(&doc)
        .ok_or_else(|| {
            warn!("No next page found");
        })
        .unwrap_or(&"");

    let urls = (0..MAX_PAGES)
        .map(|page| {
            let next_url = format!("{}{}&start={}", BASE_URL, next_page_url, page * PAGE_SIZE);
            info!("Next URL: {}", next_url);
            next_url
        })
        .collect::<Vec<_>>();

    // TODO: Remove this
    let urls = vec![urls.first().unwrap().clone()];

    let docs = join_all(
        urls.iter()
            .map(|url| get_html(&client, url))
            .collect::<Vec<_>>(),
    )
    .await;

    for doc in docs {
        posts.extend(
            get_posts_from_current_page(&(doc?).0)
                .await
                .with_context(|| format!("Failed to extract posts from page {}", page))?,
        );
    }

    // TODO: Remove this
    // // Now let's fetch the HTML for each post and store it in the Post struct
    // let post_urls = posts.keys().cloned().collect::<Vec<_>>();
    // let post_fetches = post_urls
    //     .iter()
    //     .map(|url| get_html(&client, url))
    //     .collect::<Vec<_>>();

    // for post_doc in join_all(post_fetches).await {
    let post_doc = get_html(
        &client,
        "https://www.techouvot.com/cours_par_zoom-vt8030928.html?highlight=",
    )
    .await?;
    let (doc, url) = post_doc;
    info!("Fetched HTML for post: {}", url);
    let post = posts
        .get_mut("https://www.techouvot.com/cours_par_zoom-vt8030928.html?highlight=")
        .unwrap();
    post.html = Some(doc);

    post.save()?;
    // }

    info!("Total posts found: {}", posts.len());
    Ok(())
}

async fn get_html<S>(client: &Client, url: S) -> Result<(Html, S)>
where
    S: reqwest::IntoUrl + Clone,
{
    let url_cloned = url.clone();

    let response = if url.as_str().contains("search.php?search_id") {
        client.get(url).send().await?
    } else {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );

        let days_since_2020 = number_days_since_2020();
        let body = format!("search_keywords=&search_terms=any&search_author=Rav+Binyamin+Wattenberg&search_forum=-1&search_time={days}&search_fields=all&search_cat=-1&sort_by=0&sort_dir=DESC&show_results=topics&return_chars=200",
            days = days_since_2020
        );

        client.post(url).headers(headers).body(body).send().await?
    };

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Non-success HTTP status: {}",
            response.status()
        ));
    }

    let res_bytes = response.bytes().await?;
    let response_text = match String::from_utf8(res_bytes.to_vec()) {
        Ok(text) => text,
        Err(_) => {
            // Attempt fallback encoding
            let (decoded_text, _, _) = WINDOWS_1252.decode(&res_bytes);
            decoded_text.to_string()
        }
    };

    Ok((Html::parse_document(&response_text), url_cloned))
}

async fn get_posts_from_current_page(html: &Html) -> Result<HashMap<String, Post>> {
    let mut posts = HashMap::new();

    let table_rows_selector = Selector::parse("table.forumline tr")
        .map_err(|e| anyhow::anyhow!("Failed to parse row selector: {}", e))?;

    let cells_selector = Selector::parse("td")
        .map_err(|e| anyhow::anyhow!("Failed to parse cell selector: {}", e))?;

    let link_selector = Selector::parse("a")
        .map_err(|e| anyhow::anyhow!("Failed to parse link selector: {}", e))?;

    // Skip the first row if it's a header (adjust as needed)
    let table_rows = html.select(&table_rows_selector).skip(1);

    for row in table_rows {
        // Extracting cells
        let cells: Vec<_> = row.select(&cells_selector).collect();
        if cells.len() < 7 {
            // Not a valid row
            continue;
        }

        let title_cell = &cells[2];
        let title_link = match title_cell.select(&link_selector).next() {
            Some(link) => link,
            None => {
                warn!("No title link found in cell");
                continue;
            }
        };

        let href = match title_link.value().attr("href") {
            Some(h) => h.to_string(),
            None => {
                warn!("Title link does not have href attribute");
                continue;
            }
        };

        let title = title_link.text().collect::<String>();

        posts.insert(
            format!("{}{}", BASE_URL, href),
            Post {
                title,
                ..Default::default()
            },
        );
    }

    Ok(posts)
}

fn find_next_page(html: &Html) -> Option<&str> {
    // Find the next page link
    let next_page_selector = Selector::parse(".nav a[href^=\"search.php?search_id\"]").ok()?;
    let next_page_link = html.select(&next_page_selector).next()?;

    let href = next_page_link.value().attr("href")?;
    let base_link = href.split('&').next()?;

    Some(base_link)
}
