use anyhow::{Context, Result};
use futures::future::join_all;
use reqwest::Client;
use std::collections::HashMap;
use tokio::{self};

use tracing::{info, warn, Level};

mod http;
mod parser;
mod post;
mod utils;

use http::client::{find_next_page, get_html, get_posts_from_current_page};
use utils::constants::{BASE_URL, MAX_PAGES, PAGE_SIZE};

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
        "https://www.techouvot.com/rimes_dans_la_torah-vt8030850.html?highlight=",
    )
    .await?;
    let (doc, url) = post_doc;
    info!("Fetched HTML for post: {}", url);
    let post = posts
        .get_mut("https://www.techouvot.com/rimes_dans_la_torah-vt8030850.html?highlight=")
        .unwrap();
    post.html = Some(doc);

    post.save()?;
    // }

    info!("Total posts found: {}", posts.len());
    Ok(())
}
