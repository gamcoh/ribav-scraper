use crate::utils::constants::BASE_URL;
use crate::{post::post::Post, utils::functions::number_days_since_2020};
use anyhow::Result;
use encoding_rs::WINDOWS_1252;
use reqwest::{header, Client};
use scraper::{selectable::Selectable, Html, Selector};
use std::collections::HashMap;

use tracing::warn;

pub async fn get_posts_from_current_page(html: &Html) -> Result<HashMap<String, Post>> {
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

        let category = cells[1].text().collect::<String>();

        posts.insert(
            format!("{}{}", BASE_URL, href),
            Post {
                title,
                category,
                ..Default::default()
            },
        );
    }

    Ok(posts)
}

pub fn find_next_page(html: &Html) -> Option<&str> {
    // Find the next page link
    let next_page_selector = Selector::parse(".nav a[href^=\"search.php?search_id\"]").ok()?;
    let next_page_link = html.select(&next_page_selector).next()?;

    let href = next_page_link.value().attr("href")?;
    let base_link = href.split('&').next()?;

    Some(base_link)
}

pub async fn get_html<S>(client: &Client, url: S) -> Result<(Html, S)>
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
