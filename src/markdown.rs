use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ammonia::Builder as AmmoniaBuilder;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Percent-encode set for relative local image path segments (keeps `/` as path separator).
const LOCAL_PATH_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'\\');

#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub html: String,
    pub blocked_remote_images: usize,
    pub mermaid_blocks: usize,
}

/// Renders Markdown into strictly sanitized HTML suitable for our sandboxed WebView.
///
/// Security layers:
/// 1. Extracts fenced `mermaid` code blocks as escaped text inside `<pre class="mermaid">`.
/// 2. Rewrites relative local image URLs to `/local/...` (which the protocol handler jails to the doc directory).
/// 3. Blocks remote `http(s)://` images when `allow_remote_images` is `false`.
/// 4. Sanitizes the entire HTML output via `ammonia` (strips `<script>`, `<iframe>`, `<svg>`,
///    `<style>`, `<form>`, `on*` event handlers, `javascript:` URLs, etc.).
pub fn render_markdown_safe(source: &str, allow_remote_images: bool) -> RenderOutput {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(source, options);
    let mut raw_html = String::with_capacity(source.len() * 2);

    let mut events_out: Vec<Event<'_>> = Vec::new();
    let mut in_mermaid = false;
    let mut mermaid_buf = String::new();
    let mut mermaid_blocks: usize = 0;

    // Heading tracking to generate GitHub-style slugified IDs for internal anchor links.
    let mut in_heading = false;
    let mut heading_level: Option<HeadingLevel> = None;
    let mut heading_events: Vec<Event<'_>> = Vec::new();
    let mut heading_text = String::new();
    let mut heading_explicit_id: Option<String> = None;
    let mut slug_counts: HashMap<String, usize> = HashMap::new();

    // Track blocked remote Markdown images so we can emit a clear visual placeholder.
    let mut in_blocked_image = false;
    let mut blocked_image_alt = String::new();
    let blocked_counter = Arc::new(AtomicUsize::new(0));

    for event in parser {
        if in_mermaid {
            match event {
                Event::Text(text) | Event::Code(text) => mermaid_buf.push_str(&text),
                Event::SoftBreak | Event::HardBreak => mermaid_buf.push('\n'),
                Event::End(TagEnd::CodeBlock) => {
                    in_mermaid = false;
                    mermaid_blocks += 1;
                    let escaped = escape_html(&mermaid_buf);
                    let block = format!(
                        "<div class=\"mermaid-block\"><pre class=\"mermaid\">{escaped}</pre></div>\n"
                    );
                    events_out.push(Event::Html(CowStr::Boxed(block.into_boxed_str())));
                    mermaid_buf.clear();
                }
                _ => {}
            }
            continue;
        }

        if in_blocked_image {
            match event {
                Event::Text(text) | Event::Code(text) => blocked_image_alt.push_str(&text),
                Event::End(TagEnd::Image) => {
                    in_blocked_image = false;
                    let label = if blocked_image_alt.trim().is_empty() {
                        "Remote image blocked (offline mode)".to_string()
                    } else {
                        format!(
                            "Remote image blocked: {}",
                            escape_html(blocked_image_alt.trim())
                        )
                    };
                    let placeholder = format!("<span class=\"blocked-remote-img\">{label}</span>");
                    if in_heading {
                        heading_events.push(Event::Html(CowStr::Boxed(placeholder.into_boxed_str())));
                    } else {
                        events_out.push(Event::Html(CowStr::Boxed(placeholder.into_boxed_str())));
                    }
                    blocked_image_alt.clear();
                }
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref info)))
                if is_mermaid_fence(info) =>
            {
                in_mermaid = true;
                mermaid_buf.clear();
            }
            Event::Start(Tag::Heading { level, id, .. }) => {
                in_heading = true;
                heading_level = Some(level);
                heading_events.clear();
                heading_text.clear();
                heading_explicit_id = id.map(|s| s.to_string());
            }
            Event::End(TagEnd::Heading(_)) if in_heading => {
                in_heading = false;
                let level = heading_level.take().unwrap_or(HeadingLevel::H1);
                let tag_name = heading_tag(level);
                let base_id = match heading_explicit_id.take().map(|s| s.trim().to_string()) {
                    Some(explicit) if !explicit.is_empty() => explicit,
                    _ => github_slug(&heading_text),
                };
                let slug = unique_id(base_id, &mut slug_counts);
                let escaped_slug = escape_html(&slug);

                let mut inner_html = String::new();
                pulldown_cmark::html::push_html(&mut inner_html, heading_events.drain(..));

                let full = format!("<{tag_name} id=\"{escaped_slug}\">{inner_html}</{tag_name}>\n");
                events_out.push(Event::Html(CowStr::Boxed(full.into_boxed_str())));
                heading_text.clear();
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if !allow_remote_images && is_remote_url(&dest_url) {
                    blocked_counter.fetch_add(1, Ordering::Relaxed);
                    in_blocked_image = true;
                    blocked_image_alt.clear();
                } else {
                    let rewritten = rewrite_image_url(&dest_url, allow_remote_images)
                        .unwrap_or_default();
                    let ev = Event::Start(Tag::Image {
                        link_type,
                        dest_url: CowStr::Boxed(rewritten.into_boxed_str()),
                        title,
                        id,
                    });
                    if in_heading {
                        heading_events.push(ev);
                    } else {
                        events_out.push(ev);
                    }
                }
            }
            Event::TaskListMarker(checked) => {
                let html = if checked {
                    "<span class=\"task-checkbox checked\" aria-label=\"completed\"></span>"
                } else {
                    "<span class=\"task-checkbox\" aria-label=\"incomplete\"></span>"
                };
                events_out.push(Event::Html(CowStr::Borrowed(html)));
            }
            other => {
                if in_heading {
                    if let Event::Text(ref t) | Event::Code(ref t) = other {
                        heading_text.push_str(t);
                    }
                    heading_events.push(other);
                } else {
                    events_out.push(other);
                }
            }
        }
    }

    pulldown_cmark::html::push_html(&mut raw_html, events_out.into_iter());

    // Defense-in-depth HTML sanitization via ammonia.
    let sanitized = sanitize_html(&raw_html, allow_remote_images, Arc::clone(&blocked_counter));

    RenderOutput {
        html: sanitized,
        blocked_remote_images: blocked_counter.load(Ordering::Relaxed),
        mermaid_blocks,
    }
}

fn sanitize_html(raw_html: &str, allow_remote_images: bool, blocked_counter: Arc<AtomicUsize>) -> String {
    let mut tags: HashSet<&str> = HashSet::from([
        "h1", "h2", "h3", "h4", "h5", "h6", "p", "br", "hr", "strong", "b", "em", "i", "u",
        "del", "s", "code", "pre", "blockquote", "ul", "ol", "li", "table", "thead", "tbody",
        "tfoot", "tr", "th", "td", "a", "img", "div", "span", "sup", "sub", "details", "summary",
        "dl", "dt", "dd", "kbd", "mark", "abbr", "section", "figure", "figcaption", "caption",
    ]);
    // Explicitly never include script, style, iframe, object, embed, form, input, svg, math, meta, link, base.
    tags.remove("script");

    let mut builder = AmmoniaBuilder::empty();
    builder
        .tags(tags)
        .add_tag_attributes("a", &["href", "title"])
        .add_tag_attributes("img", &["src", "alt", "title", "width", "height"])
        .add_tag_attributes("th", &["align", "colspan", "rowspan", "style"])
        .add_tag_attributes("td", &["align", "colspan", "rowspan", "style"])
        .add_tag_attributes("ol", &["start"])
        .add_tag_attributes("details", &["open"])
        .add_tag_attributes("abbr", &["title"])
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("h5", &["id"])
        .add_tag_attributes("h6", &["id"])
        .add_tag_attributes("div", &["align", "id"])
        .add_tag_attributes("p", &["align"])
        .add_tag_attributes("section", &["id"])
        .add_tag_attributes("sup", &["id"])
        .add_tag_attributes("li", &["id"])
        .add_tag_attributes("span", &["aria-label"])
        // Allow only specific safe classes (code language classes are filtered via attribute_filter below).
        .add_allowed_classes("div", &["mermaid-block", "footnotes", "footnote-definition"])
        .add_allowed_classes("sup", &["footnote-reference", "footnote-definition-label"])
        .add_allowed_classes("pre", &["mermaid"])
        .add_allowed_classes(
            "span",
            &["task-checkbox", "checked", "blocked-remote-img"],
        )
        .add_tag_attributes("code", &["class"])
        .url_schemes(HashSet::from(["http", "https", "mailto", "data"]))
        .url_relative(ammonia::UrlRelative::PassThrough)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .attribute_filter(move |element, attribute, value| {
            // Allow only exact table text-align styles generated by pulldown-cmark.
            if (element == "th" || element == "td") && attribute == "style" {
                let normalized = value.trim().trim_end_matches(';').trim().to_ascii_lowercase();
                return match normalized.as_str() {
                    "text-align: left" | "text-align:left" => Some("text-align: left".into()),
                    "text-align: center" | "text-align:center" => Some("text-align: center".into()),
                    "text-align: right" | "text-align:right" => Some("text-align: right".into()),
                    _ => None,
                };
            }

            // Restrict <code> class to safe `language-*` identifiers only.
            if element == "code" && attribute == "class" {
                let valid = value.split_whitespace().all(|cls| {
                    cls.starts_with("language-")
                        && cls[9..]
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' || c == '#')
                });
                return if valid { Some(value.into()) } else { None };
            }

            // Strictly validate and rewrite <img src="..."> even for raw HTML <img> tags.
            if element == "img" && attribute == "src" {
                if !allow_remote_images && is_remote_url(value) {
                    blocked_counter.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
                return rewrite_image_url(value, allow_remote_images).map(Into::into);
            }

            // Disallow data: URLs in <a href> (only allow http/https/mailto/relative/#anchor).
            if element == "a" && attribute == "href" {
                let trimmed = value.trim();
                let lower = trimmed.to_ascii_lowercase();
                if lower.starts_with("data:")
                    || lower.starts_with("javascript:")
                    || lower.starts_with("vbscript:")
                    || lower.starts_with("file:")
                {
                    return None;
                }
                return Some(trimmed.into());
            }

            Some(value.into())
        });

    builder.clean(raw_html).to_string()
}

fn is_mermaid_fence(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .map(|lang| lang.eq_ignore_ascii_case("mermaid"))
        .unwrap_or(false)
}

fn is_remote_url(url: &str) -> bool {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("//")
}

/// Validates and rewrites an image URL:
/// - Already rewritten `/local/...` -> pass through (validated again by protocol handler)
/// - `http://` / `https://` -> allowed only when `allow_remote_images == true`
/// - `data:image/(png|jpeg|jpg|gif|webp|avif);base64,...` -> allowed (no SVG data URIs)
/// - Relative local paths -> `/local/<percent-encoded-path>`
/// - Anything else (`file://`, `javascript:`, absolute FS paths, `//`) -> `None`
fn rewrite_image_url(url: &str, allow_remote_images: bool) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("/local/") {
        return Some(trimmed.to_string());
    }

    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("http://") || lower.starts_with("https://") {
        return if allow_remote_images {
            Some(trimmed.to_string())
        } else {
            None
        };
    }

    if lower.starts_with("data:") {
        let safe_raster_data = lower.starts_with("data:image/png;base64,")
            || lower.starts_with("data:image/jpeg;base64,")
            || lower.starts_with("data:image/jpg;base64,")
            || lower.starts_with("data:image/gif;base64,")
            || lower.starts_with("data:image/webp;base64,")
            || lower.starts_with("data:image/avif;base64,");
        return if safe_raster_data {
            Some(trimmed.to_string())
        } else {
            None
        };
    }

    // Reject protocol-relative URLs, explicit schemes (file:, javascript:, etc.), and absolute paths.
    if trimmed.starts_with("//") || trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return None;
    }
    if let Some(colon_idx) = trimmed.find(':') {
        let slash_idx = trimmed.find('/').unwrap_or(usize::MAX);
        if colon_idx < slash_idx {
            return None;
        }
    }

    // Normalize relative path (strip leading `./`, normalize Windows `\` to `/`).
    let normalized = trimmed.replace('\\', "/");
    let stripped = normalized.trim_start_matches("./");
    if stripped.is_empty() {
        return None;
    }

    let encoded = utf8_percent_encode(stripped, LOCAL_PATH_ENCODE_SET).to_string();
    Some(format!("/local/{encoded}"))
}

fn heading_tag(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "h1",
        HeadingLevel::H2 => "h2",
        HeadingLevel::H3 => "h3",
        HeadingLevel::H4 => "h4",
        HeadingLevel::H5 => "h5",
        HeadingLevel::H6 => "h6",
    }
}

fn github_slug(text: &str) -> String {
    let mut base = String::with_capacity(text.len());
    for ch in text.trim().chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                base.push(lower);
            }
        } else if ch == '-' || ch == '_' {
            base.push(ch);
        } else if ch.is_whitespace() {
            base.push('-');
        }
    }
    if base.is_empty() {
        "section".to_string()
    } else {
        base
    }
}

fn unique_id(base: String, counts: &mut HashMap<String, usize>) -> String {
    let entry = counts.entry(base.clone()).or_insert(0);
    let slug = if *entry == 0 {
        base
    } else {
        format!("{base}-{}", *entry)
    };
    *entry += 1;
    slug
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_xss_vectors() {
        let md = r#"
# Hello

<script>alert('xss')</script>
<img src=x onerror="alert(1)">
<svg onload="alert(2)"><script>alert(3)</script></svg>
<iframe src="https://evil.example"></iframe>

[click me](javascript:alert(4))

<a href="javascript:alert(7)">raw js link</a>
<a href="data:text/html,<script>alert(5)</script>">bad</a>
<div onclick="alert(6)">safe text</div>
"#;
        let out = render_markdown_safe(md, false);
        assert!(!out.html.contains("<script"), "html: {}", out.html);
        assert!(!out.html.contains("onerror"), "html: {}", out.html);
        assert!(!out.html.contains("onload"), "html: {}", out.html);
        assert!(!out.html.contains("onclick"), "html: {}", out.html);
        assert!(!out.html.contains("<iframe"), "html: {}", out.html);
        assert!(!out.html.contains("<svg"), "html: {}", out.html);
        assert!(!out.html.contains("javascript:"), "html: {}", out.html);
        assert!(!out.html.contains("data:text/html"), "html: {}", out.html);
        assert!(out.html.contains("safe text"), "html: {}", out.html);
    }

    #[test]
    fn extracts_and_escapes_mermaid_blocks() {
        let md = "```mermaid\ngraph TD\n    A[\"<script>alert(1)</script>\"] --> B\n```";
        let out = render_markdown_safe(md, false);
        assert_eq!(out.mermaid_blocks, 1);
        assert!(out.html.contains("<pre class=\"mermaid\">"));
        assert!(out.html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!out.html.contains("<script>"));
    }

    #[test]
    fn blocks_remote_images_by_default_and_allows_when_enabled() {
        let md = "![Logo](https://example.com/logo.png)\n\n<img src=\"https://evil.com/track.gif\">";
        let blocked = render_markdown_safe(md, false);
        assert_eq!(blocked.blocked_remote_images, 2);
        assert!(!blocked.html.contains("https://example.com/logo.png"));
        assert!(!blocked.html.contains("https://evil.com/track.gif"));
        assert!(blocked.html.contains("blocked-remote-img"));

        let allowed = render_markdown_safe(md, true);
        assert_eq!(allowed.blocked_remote_images, 0);
        assert!(allowed.html.contains("https://example.com/logo.png"));
        assert!(allowed.html.contains("https://evil.com/track.gif"));
    }

    #[test]
    fn rewrites_local_relative_images_and_rejects_file_scheme() {
        let md = "![Local](<./images/my diagram.png>)\n![Simple](assets/pic.png)\n![Bad](file:///etc/passwd)";
        let out = render_markdown_safe(md, false);
        assert!(out.html.contains("/local/images/my%20diagram.png"), "html: {}", out.html);
        assert!(out.html.contains("/local/assets/pic.png"), "html: {}", out.html);
        assert!(!out.html.contains("file:///etc/passwd"), "html: {}", out.html);
    }

    #[test]
    fn generates_github_compatible_and_explicit_heading_ids() {
        let md = "# Intro\n## Setup\n## Setup\n## Tables & Code\n## foo_bar\n## Custom {#mi_id}\n";
        let out = render_markdown_safe(md, false);
        assert!(out.html.contains("<h1 id=\"intro\">Intro</h1>"), "html: {}", out.html);
        assert!(out.html.contains("<h2 id=\"setup\">Setup</h2>"), "html: {}", out.html);
        assert!(out.html.contains("<h2 id=\"setup-1\">Setup</h2>"), "html: {}", out.html);
        assert!(out.html.contains("<h2 id=\"tables--code\">Tables &amp; Code</h2>"), "html: {}", out.html);
        assert!(out.html.contains("<h2 id=\"foo_bar\">foo_bar</h2>"), "html: {}", out.html);
        assert!(out.html.contains("<h2 id=\"mi_id\">Custom</h2>"), "html: {}", out.html);
    }

    #[test]
    fn preserves_table_alignment_and_blocks_unsafe_styles() {
        let md = "| L | C | R |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |\n\n<table><tr><td style=\"background:url(evil)\">x</td></tr></table>";
        let out = render_markdown_safe(md, false);
        assert!(out.html.contains("style=\"text-align: left\""), "html: {}", out.html);
        assert!(out.html.contains("style=\"text-align: center\""), "html: {}", out.html);
        assert!(out.html.contains("style=\"text-align: right\""), "html: {}", out.html);
        assert!(!out.html.contains("background:url"), "html: {}", out.html);
    }

    #[test]
    fn preserves_footnote_classes() {
        let md = "Text with footnote[^1].\n\n[^1]: Footnote body.\n";
        let out = render_markdown_safe(md, false);
        assert!(out.html.contains("class=\"footnote-reference\""), "html: {}", out.html);
        assert!(out.html.contains("class=\"footnote-definition\""), "html: {}", out.html);
        assert!(out.html.contains("class=\"footnote-definition-label\""), "html: {}", out.html);
    }
}
