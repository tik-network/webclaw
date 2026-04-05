/// Reddit JSON API fallback for extracting posts + comments without JS rendering.
///
/// Handles all Reddit page types: post pages, subreddit listings, comment feeds,
/// user profiles, search results, etc. Appending `.json` to any Reddit URL
/// returns structured JSON which we convert to clean markdown.
use serde::Deserialize;
use tracing::debug;
use webclaw_core::{Content, ExtractionResult, Metadata};

/// Check if a URL points to a Reddit page.
pub fn is_reddit_url(url: &str) -> bool {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("");
    matches!(
        host,
        "reddit.com" | "www.reddit.com" | "old.reddit.com" | "np.reddit.com" | "new.reddit.com"
    )
}

/// Build the `.json` URL from a Reddit page URL.
pub fn json_url(url: &str) -> String {
    let clean = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    format!("{clean}.json")
}

/// Convert Reddit JSON API response into an ExtractionResult.
/// Generic: handles arrays (post pages), single objects (listings), any mix of t1/t3/t5.
pub fn parse_reddit_json(json_bytes: &[u8], url: &str) -> Result<ExtractionResult, String> {
    let listings: Vec<Listing> = serde_json::from_slice(json_bytes).or_else(|_| {
        let single: Listing =
            serde_json::from_slice(json_bytes).map_err(|e| format!("reddit json parse: {e}"))?;
        Ok::<_, String>(vec![single])
    })?;

    let mut markdown = String::new();
    let mut title = None;
    let mut author = None;
    let mut subreddit = None;

    for listing in &listings {
        for child in &listing.data.children {
            match child.kind.as_str() {
                "t3" => render_post(child, &mut markdown, &mut title, &mut author, &mut subreddit),
                "t1" => render_comment(child, 0, &mut markdown),
                _ => {}
            }
        }
    }

    let word_count = markdown.split_whitespace().count();
    debug!(word_count, "reddit json extracted");

    Ok(ExtractionResult {
        metadata: Metadata {
            title,
            description: None,
            author,
            published_date: None,
            language: Some("en".into()),
            url: Some(url.to_string()),
            site_name: subreddit,
            image: None,
            favicon: None,
            word_count,
        },
        content: Content {
            markdown,
            plain_text: String::new(),
            links: vec![],
            images: vec![],
            code_blocks: vec![],
            raw_html: None,
        },
        domain_data: None,
        structured_data: vec![],
    })
}

/// Render a post (t3). If it's the first post, use it as the page title.
fn render_post(
    thing: &Thing,
    out: &mut String,
    page_title: &mut Option<String>,
    page_author: &mut Option<String>,
    page_subreddit: &mut Option<String>,
) {
    let d = &thing.data;
    let t = d.title.as_deref().unwrap_or("[untitled]");
    let a = d.author.as_deref().unwrap_or("[deleted]");
    let sr = d.subreddit_name_prefixed.as_deref().unwrap_or("");
    let score = d.score.unwrap_or(0);

    // First post sets page metadata
    if page_title.is_none() {
        *page_title = d.title.clone();
        *page_author = d.author.clone();
        *page_subreddit = d.subreddit_name_prefixed.clone();
    }

    out.push_str(&format!("### {t}\n\n"));
    out.push_str(&format!("**u/{a}** in {sr} ({score} pts)\n\n"));

    if let Some(ref body) = d.selftext
        && !body.is_empty()
    {
        out.push_str(body);
        out.push_str("\n\n");
    }
    if let Some(ref link) = d.url_overridden_by_dest
        && !link.is_empty()
    {
        out.push_str(&format!("[Link]({link})\n\n"));
    }
    out.push_str("---\n\n");
}

/// Render a comment (t1) with indentation for nesting.
fn render_comment(thing: &Thing, depth: usize, out: &mut String) {
    if thing.kind != "t1" {
        return;
    }
    let d = &thing.data;
    let indent = "  ".repeat(depth);
    let author = d.author.as_deref().unwrap_or("[deleted]");
    let body = d.body.as_deref().unwrap_or("[removed]");
    let score = d.score.unwrap_or(0);

    // Show which post this comment is on (for comment feed pages)
    if depth == 0 {
        if let Some(ref link_title) = d.link_title {
            out.push_str(&format!("**Re: {link_title}**\n"));
        }
    }

    out.push_str(&format!("{indent}- **u/{author}** ({score} pts)\n"));
    for line in body.lines() {
        out.push_str(&format!("{indent}  {line}\n"));
    }
    out.push('\n');

    if let Some(Replies::Listing(listing)) = &d.replies {
        for child in &listing.data.children {
            render_comment(child, depth + 1, out);
        }
    }
}

// --- Reddit JSON types (minimal, permissive) ---

#[derive(Deserialize)]
struct Listing {
    data: ListingData,
}

#[derive(Deserialize)]
struct ListingData {
    children: Vec<Thing>,
}

#[derive(Deserialize)]
struct Thing {
    kind: String,
    data: ThingData,
}

#[derive(Deserialize)]
struct ThingData {
    // Post fields (t3)
    title: Option<String>,
    selftext: Option<String>,
    subreddit_name_prefixed: Option<String>,
    url_overridden_by_dest: Option<String>,
    // Comment fields (t1)
    author: Option<String>,
    body: Option<String>,
    score: Option<i64>,
    replies: Option<Replies>,
    /// Title of the parent post (present on comments in feed pages)
    link_title: Option<String>,
}

/// Reddit replies can be either a nested Listing or an empty string.
#[derive(Deserialize)]
#[serde(untagged)]
enum Replies {
    Listing(Listing),
    #[allow(dead_code)]
    Empty(String),
}
