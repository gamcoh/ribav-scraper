use chrono::{TimeZone, Utc};
use reqwest::{header, Client};
use scraper::Html;
use std::collections::HashMap;
use std::error::Error;

fn number_days_since_2020() -> i64 {
    let today = Utc::now();
    let start_of_2020 = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();

    let duration = today - start_of_2020;

    duration.num_days()
}

#[derive(Debug)]
struct Post {
    title: String,
    forum: String,
    responses: u32,
    views: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let client = Client::builder().cookie_store(true).build()?;
    let url = "https://www.techouvot.com/search.php?mode=results";
    let base_url = "https://www.techouvot.com/";
    let mut posts = HashMap::new();

    let mut page = 0;

    let mut doc = get_html(&client, url).await.expect("Failed to get HTML");
    while let Some(next_page_url) = find_next_page(&doc, page) {
        posts.extend(
            get_posts_from_current_page(&doc)
                .await
                .expect("Failed to get posts"),
        );

        let next_url = format!("{}{}", base_url, next_page_url);
        println!("Next URL: {}", next_url);
        doc = get_html(&client, &next_url)
            .await
            .expect("Failed to get HTML");
        page += 1;

        if page > 5 {
            break;
        }
    }

    dbg!(&posts.len());

    Ok(())
}

async fn get_html(client: &Client, url: &str) -> Result<Html, Box<dyn Error>> {
    let response;
    if url.contains("search.php?search_id") {
        response = client.get(url).send().await?;
    } else {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );

        let days_since_2020 = number_days_since_2020();
        let body = format!( "search_keywords=&search_terms=any&search_author=Rav+Binyamin+Wattenberg&search_forum=-1&search_time={}&search_fields=all&search_cat=-1&sort_by=0&sort_dir=DESC&show_results=topics&return_chars=200", days_since_2020);

        response = client.post(url).headers(headers).body(body).send().await?;
    }

    assert!(response.status().is_success());
    let res_bytes = response.bytes().await?;
    let response_text = match String::from_utf8(res_bytes.to_vec()) {
        Ok(text) => text,
        Err(_) => {
            // Fallback: Assume the response might have a different encoding
            use encoding_rs::WINDOWS_1252;
            let (decoded_text, _, _) = WINDOWS_1252.decode(&res_bytes);
            decoded_text.to_string()
        }
    };

    Ok(Html::parse_document(&response_text))
}

async fn get_posts_from_current_page(html: &Html) -> Result<HashMap<String, Post>, Box<dyn Error>> {
    let mut posts = HashMap::new();

    let table_rows_selector = scraper::Selector::parse("table.forumline tr")?;

    let table_rows = html.select(&table_rows_selector);
    for row in table_rows.skip(1) {
        let cells_selector = scraper::Selector::parse("td")?;
        let [_, forum_cell, title_cell, _, responses_cell, views_cell, _] =
            row.select(&cells_selector).collect::<Vec<_>>()[..]
        else {
            continue;
        };

        let link_selector = scraper::Selector::parse("a")?;
        let forum_link = forum_cell.select(&link_selector).next().unwrap();
        let title_link = title_cell.select(&link_selector).next().unwrap();
        let responses = responses_cell.text().collect::<String>().parse::<u32>()?;
        let views = views_cell.text().collect::<String>().parse::<u32>()?;

        posts.insert(
            title_link.attr("href").unwrap().to_string(),
            Post {
                title: title_link.text().collect(),
                forum: forum_link.text().collect(),
                responses,
                views,
            },
        );
    }

    Ok(posts)
}

fn find_next_page(html: &Html, page: u8) -> Option<String> {
    let next_page_selector =
        scraper::Selector::parse(".nav a[href^=\"search.php?search_id\"]").ok()?;
    let next_page_link = html.select(&next_page_selector).next()?;

    // Remove the `&start=` query parameter from the link
    let next_page_link = next_page_link.value().attr("href")?;
    let next_page_link = next_page_link.split('&').next()?;

    Some(format!("{}&start={}", next_page_link, page * 50))
}
