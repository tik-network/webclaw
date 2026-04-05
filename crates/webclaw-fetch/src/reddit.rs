/// Reddit JSON API fallback for extracting posts + comments without JS rendering.
///
/// Reddit's new `shreddit` frontend only SSRs the post body — comments are
/// loaded client-side. Appending `.json` to any Reddit URL returns the full
/// comment tree as structured JSON, which we convert to clean markdown.
use serde::Deserialize;
use tracing::debug;
use webclaw_core::{Content, ExtractionResult, Metadata};

/// Check if a URL points to a Reddit post/comment page.
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
/// Handles both post pages (JSON array) and subreddit/listing pages (single JSON object).
pub fn parse_reddit_json(json_bytes: &[u8], url: &str) -> Result<ExtractionResult, String> {
    // Post pages return [post_listing, comment_listing], subreddit pages return a single listing.
    let listings: Vec<Listing> = serde_json::from_slice(json_bytes).or_else(|_| {
        let single: Listing =
            serde_json::from_slice(json_bytes).map_err(|e| format!("reddit json parse: {e}"))?;
        Ok::<_, String>(vec![single])
    })?;

    let mut markdown = String::new();
    let mut title = None;
    let mut author = None;
    let mut subreddit = None;

    let is_listing_page = listings.len() == 1;

    // First listing = post(s)
    if let Some(post_listing) = listings.first() {
        let posts: Vec<_> = post_listing
            .data
            .children
            .iter()
            .filter(|c| c.kind == "t3")
            .collect();

        if is_listing_page && posts.len() > 1 {
            // Subreddit listing: render as a list of posts
            subreddit = posts
                .first()
                .and_then(|p| p.data.subreddit_name_prefixed.clone());
            if let Some(ref sr) = subreddit {
                markdown.push_str(&format!("# {sr}\n\n"));
            }
            for post in &posts {
                let d = &post.data;
                let t = d.title.as_deref().unwrap_or("[untitled]");
                let a = d.author.as_deref().unwrap_or("[deleted]");
                let score = d.score.unwrap_or(0);
                markdown.push_str(&format!("- **{t}** — u/{a} ({score} pts)\n"));
                if let Some(ref body) = d.selftext
                    && !body.is_empty()
                {
                    let preview: String = body.chars().take(200).collect();
                    markdown.push_str(&format!("  {preview}\n"));
                }
                if let Some(ref link) = d.url_overridden_by_dest
                    && !link.is_empty()
                {
                    markdown.push_str(&format!("  [Link]({link})\n"));
                }
                markdown.push('\n');
            }
        } else {
            // Single post page
            for post in &posts {
                let d = &post.data;
                title = d.title.clone();
                author = d.author.clone();
                subreddit = d.subreddit_name_prefixed.clone();

                if let Some(ref t) = title {
                    markdown.push_str(&format!("# {t}\n\n"));
                }
                if let (Some(a), Some(sr)) = (&author, &subreddit) {
                    markdown.push_str(&format!("**u/{a}** in {sr}\n\n"));
                }
                if let Some(ref body) = d.selftext
                    && !body.is_empty()
                {
                    markdown.push_str(body);
                    markdown.push_str("\n\n");
                }
                if let Some(ref url_field) = d.url_overridden_by_dest
                    && !url_field.is_empty()
                {
                    markdown.push_str(&format!("[Link]({url_field})\n\n"));
                }
                markdown.push_str("---\n\n");
            }
        }
    }

    // Second listing = comment tree (only on post pages)
    if let Some(comment_listing) = listings.get(1) {
        markdown.push_str("## Comments\n\n");
        for child in &comment_listing.data.children {
            render_comment(child, 0, &mut markdown);
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

fn render_comment(thing: &Thing, depth: usize, out: &mut String) {
    if thing.kind != "t1" {
        return;
    }
    let d = &thing.data;
    let indent = "  ".repeat(depth);
    let author = d.author.as_deref().unwrap_or("[deleted]");
    let body = d.body.as_deref().unwrap_or("[removed]");
    let score = d.score.unwrap_or(0);

    out.push_str(&format!("{indent}- **u/{author}** ({score} pts)\n"));
    for line in body.lines() {
        out.push_str(&format!("{indent}  {line}\n"));
    }
    out.push('\n');

    // Recurse into replies
    if let Some(Replies::Listing(listing)) = &d.replies {
        for child in &listing.data.children {
            render_comment(child, depth + 1, out);
        }
    }
}

// --- Reddit JSON types (minimal) ---

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
}

/// Reddit replies can be either a nested Listing or an empty string.
#[derive(Deserialize)]
#[serde(untagged)]
enum Replies {
    Listing(Listing),
    #[allow(dead_code)]
    Empty(String),
}
