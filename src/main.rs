use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use encoding_rs::WINDOWS_1252;
use futures::future::join_all;
use reqwest::{header, Client};
use scraper::{Html, Selector};
use std::collections::HashMap;
use tokio::{self};

use tracing::{info, warn, Level};

const MAX_PAGES: u16 = 1;
const PAGE_SIZE: u16 = 50;
const BASE_URL: &str = "https://www.techouvot.com/";

#[derive(Debug, Default)]
struct Post {
    title: String,
    forum: String,
    responses: u32,
    views: u32,
    html: Option<Html>,
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

    let next_page_url = find_next_page(&doc).ok_or_else(|| {
        warn!("No next page found");
        anyhow::anyhow!("No next page found")
    })?;

    let urls = (0..MAX_PAGES)
        .map(|page| {
            let next_url = format!("{}{}&start={}", BASE_URL, next_page_url, page * PAGE_SIZE);
            info!("Next URL: {}", next_url);
            next_url
        })
        .collect::<Vec<_>>();

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

    // Now let's fetch the HTML for each post and store it in the Post struct
    let post_urls = posts.keys().cloned().collect::<Vec<_>>();
    let post_fetches = post_urls
        .iter()
        .map(|url| get_html(&client, url))
        .collect::<Vec<_>>();

    for post_doc in join_all(post_fetches).await {
        let (doc, url) = post_doc?;
        info!("Fetched HTML for post: {}", url);
        let post = posts.get_mut((*url).as_str()).unwrap();
        post.html = Some(doc);
    }

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

        let forum_cell = &cells[1];
        let title_cell = &cells[2];
        let responses_cell = &cells[4];
        let views_cell = &cells[5];

        let forum_link = match forum_cell.select(&link_selector).next() {
            Some(link) => link,
            None => {
                warn!("No forum link found in cell");
                continue;
            }
        };
        let title_link = match title_cell.select(&link_selector).next() {
            Some(link) => link,
            None => {
                warn!("No title link found in cell");
                continue;
            }
        };

        let responses_text = responses_cell.text().collect::<String>();
        let views_text = views_cell.text().collect::<String>();

        let responses = responses_text
            .parse::<u32>()
            .with_context(|| format!("Failed to parse responses: {}", responses_text))?;
        let views = views_text
            .parse::<u32>()
            .with_context(|| format!("Failed to parse views: {}", views_text))?;

        let href = match title_link.value().attr("href") {
            Some(h) => h.to_string(),
            None => {
                warn!("Title link does not have href attribute");
                continue;
            }
        };

        let title = title_link.text().collect::<String>();
        let forum = forum_link.text().collect::<String>();

        posts.insert(
            format!("{}{}", BASE_URL, href),
            Post {
                title,
                forum,
                responses,
                views,
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

    // Add &start parameter to navigate pages
    Some(base_link)
}
