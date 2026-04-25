use lol_html::html_content::ContentType;
use lol_html::{HtmlRewriter, Settings, element};
use regex::Regex;
use std::sync::OnceLock;
use url::Url;

pub const DOM_PROXY_SCRIPT: &str = include_str!("dom_proxy.js");

pub fn proxy_endpoint() -> String {
    format!(
        "http://{}:{}/asset",
        common::SERVICE_HOST,
        common::SERVICE_PORT
    )
}

pub fn rewrite_html(html: &str, page_url: &str) -> Result<String, String> {
    let proxify = |raw: &str| proxy_url(raw, page_url);
    let proxify_keep_fragment = |raw: &str| proxy_url_keep_fragment(raw, page_url);

    // 1) Strip todos los <script> de la página. Camoufox ya corrió el JS
    //    cuando rendereó; lo que llega al iframe es un DOM estático. Re-correr
    //    JS de SPAs (Bloks, React, etc.) en el iframe sandbox rompe porque les
    //    faltan APIs / cookies / origen. Nuestro propio shim se inyecta después
    //    via head_block (ContentType::Html) y NO pasa por este strip.
    let pre0 = strip_page_scripts(html);
    // 2) Reescribir url() dentro de bloques <style>.
    let pre = rewrite_inline_style_blocks(&pre0, page_url).into_owned();
    let mut output: Vec<u8> = Vec::with_capacity(pre.len() + 4096);

    let head_block = head_block(page_url);
    let head_block_ref = head_block.as_str();

    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!(
                    "img, script, iframe, frame, source, video, audio, track, embed",
                    |el| {
                        if let Some(src) = el.get_attribute("src") {
                            if let Some(p) = proxify(&src) {
                                let _ = el.set_attribute("src", &p);
                            }
                        }
                        if let Some(srcset) = el.get_attribute("srcset") {
                            let new_ss = rewrite_srcset(&srcset, page_url);
                            if new_ss != srcset {
                                let _ = el.set_attribute("srcset", &new_ss);
                            }
                        }
                        if let Some(poster) = el.get_attribute("poster") {
                            if let Some(p) = proxify(&poster) {
                                let _ = el.set_attribute("poster", &p);
                            }
                        }
                        Ok(())
                    }
                ),
                element!("link", |el| {
                    if let Some(href) = el.get_attribute("href") {
                        if let Some(p) = proxify(&href) {
                            let _ = el.set_attribute("href", &p);
                        }
                    }
                    Ok(())
                }),
                element!("input[type='image']", |el| {
                    if let Some(src) = el.get_attribute("src") {
                        if let Some(p) = proxify(&src) {
                            let _ = el.set_attribute("src", &p);
                        }
                    }
                    Ok(())
                }),
                // SVG: <use> y <image> con href / xlink:href, conservando #fragment.
                element!("use, image", |el| {
                    for attr in &["href", "xlink:href"] {
                        if let Some(v) = el.get_attribute(attr) {
                            if let Some(p) = proxify_keep_fragment(&v) {
                                let _ = el.set_attribute(attr, &p);
                            }
                        }
                    }
                    Ok(())
                }),
                element!("object", |el| {
                    if let Some(v) = el.get_attribute("data") {
                        if let Some(p) = proxify(&v) {
                            let _ = el.set_attribute("data", &p);
                        }
                    }
                    Ok(())
                }),
                // Atributo style="..." con url() inline en cualquier elemento.
                element!("[style]", |el| {
                    if let Some(s) = el.get_attribute("style") {
                        let decoded = decode_basic_entities(&s);
                        let new_s = rewrite_css(&decoded, page_url);
                        if new_s != decoded {
                            let _ = el.set_attribute("style", &new_s);
                        }
                    }
                    Ok(())
                }),
                // Reescribe el valor legacy "origin-when-crossorigin" (sin guion)
                // que Facebook y otros sitios viejos siguen usando, al estándar
                // moderno "origin-when-cross-origin". WebKit rechaza el legacy.
                element!("meta[name='referrer']", |el| {
                    if let Some(v) = el.get_attribute("content") {
                        if v.eq_ignore_ascii_case("origin-when-crossorigin") {
                            let _ = el.set_attribute("content", "origin-when-cross-origin");
                        }
                    }
                    Ok(())
                }),
                element!("[referrerpolicy]", |el| {
                    if let Some(v) = el.get_attribute("referrerpolicy") {
                        if v.eq_ignore_ascii_case("origin-when-crossorigin") {
                            let _ = el.set_attribute("referrerpolicy", "origin-when-cross-origin");
                        }
                    }
                    Ok(())
                }),
                element!("head", move |el| {
                    el.prepend(head_block_ref, ContentType::Html);
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| output.extend_from_slice(c),
    );

    rewriter
        .write(pre.as_bytes())
        .map_err(|e| format!("rewrite write: {e}"))?;
    rewriter.end().map_err(|e| format!("rewrite end: {e}"))?;

    let mut out = String::from_utf8(output).map_err(|e| format!("utf8: {e}"))?;
    if !out.contains("__sandbox_head_marker__") {
        // Page had no <head>; fallback inject at start.
        out = format!("{head_block}{out}");
    }
    Ok(out)
}

pub fn rewrite_css(css: &str, css_url: &str) -> String {
    static RE_URL: OnceLock<Regex> = OnceLock::new();
    static RE_IMPORT: OnceLock<Regex> = OnceLock::new();
    static RE_SOURCEMAP: OnceLock<Regex> = OnceLock::new();
    let re_url = RE_URL
        .get_or_init(|| Regex::new(r#"url\(\s*(?:'([^']*)'|"([^"]*)"|([^)\s]+))\s*\)"#).unwrap());
    let re_import = RE_IMPORT
        .get_or_init(|| Regex::new(r#"@import\s+(?:'([^']*)'|"([^"]*)")"#).unwrap());
    let re_sourcemap = RE_SOURCEMAP.get_or_init(|| {
        Regex::new(r"(?m)/\*[#@]\s*sourceMappingURL=[^*]*\*+(?:[^/*][^*]*\*+)*/").unwrap()
    });

    let stripped = re_sourcemap.replace_all(css, "");
    let pass1 = re_url.replace_all(&stripped, |caps: &regex::Captures| {
        let url = caps
            .get(1)
            .or_else(|| caps.get(2))
            .or_else(|| caps.get(3))
            .map(|m| m.as_str())
            .unwrap_or("");
        match proxy_url(url, css_url) {
            Some(p) => format!("url(\"{p}\")"),
            None => caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
        }
    });
    let pass2 = re_import.replace_all(&pass1, |caps: &regex::Captures| {
        let url = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        match proxy_url(url, css_url) {
            Some(p) => format!("@import \"{p}\""),
            None => caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
        }
    });
    pass2.into_owned()
}

fn rewrite_srcset(srcset: &str, page_url: &str) -> String {
    srcset
        .split(',')
        .map(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                return String::new();
            }
            let (url, descriptor) = match trimmed.split_once(char::is_whitespace) {
                Some((u, d)) => (u, format!(" {}", d.trim_start())),
                None => (trimmed, String::new()),
            };
            match proxy_url(url, page_url) {
                Some(p) => format!("{p}{descriptor}"),
                None => format!("{url}{descriptor}"),
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn proxy_url(raw: &str, base: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("data:")
        || lower.starts_with("blob:")
        || lower.starts_with("javascript:")
        || lower.starts_with("about:")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || trimmed.starts_with('#')
    {
        return None;
    }
    let abs = absolutize(trimmed, base)?;
    if abs.starts_with(&proxy_endpoint()) {
        return None;
    }
    Some(format!(
        "{endpoint}?u={u}&t={t}",
        endpoint = proxy_endpoint(),
        u = urlencoding::encode(&abs),
        t = urlencoding::encode(common::APP_TOKEN),
    ))
}

/// Como `proxy_url` pero preserva el `#fragment` fuera de la URL del proxy.
/// Necesario para `<use href="sprite.svg#icon-id">` donde el fragmento
/// identifica el símbolo dentro del SVG y no debe ir codificado dentro de `u=`.
fn proxy_url_keep_fragment(raw: &str, base: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (url_part, fragment) = match trimmed.find('#') {
        Some(i) => (&trimmed[..i], &trimmed[i..]),
        None => (trimmed, ""),
    };
    if url_part.is_empty() {
        return None;
    }
    let proxied = proxy_url(url_part, base)?;
    if fragment.is_empty() {
        Some(proxied)
    } else {
        Some(format!("{proxied}{fragment}"))
    }
}

fn strip_page_scripts(html: &str) -> String {
    static RE_PAIR: OnceLock<Regex> = OnceLock::new();
    static RE_SELF: OnceLock<Regex> = OnceLock::new();
    let re_pair = RE_PAIR
        .get_or_init(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").unwrap());
    let re_self =
        RE_SELF.get_or_init(|| Regex::new(r"(?i)<script\b[^>]*/>").unwrap());
    let pass1 = re_pair.replace_all(html, "");
    let pass2 = re_self.replace_all(&pass1, "");
    pass2.into_owned()
}

fn rewrite_inline_style_blocks<'a>(html: &'a str, base_url: &str) -> std::borrow::Cow<'a, str> {
    static RE_STYLE: OnceLock<Regex> = OnceLock::new();
    let re = RE_STYLE
        .get_or_init(|| Regex::new(r"(?is)(<style\b[^>]*>)(.*?)(</style>)").unwrap());
    re.replace_all(html, |caps: &regex::Captures| {
        let open = &caps[1];
        let body = &caps[2];
        let close = &caps[3];
        let new_body = rewrite_css(body, base_url);
        format!("{open}{new_body}{close}")
    })
}

fn absolutize(url_or_path: &str, base: &str) -> Option<String> {
    let parsed_base = Url::parse(base).ok()?;
    parsed_base.join(url_or_path).ok().map(|u| u.to_string())
}

fn head_block(page_url: &str) -> String {
    let token_js = serde_json::to_string(common::APP_TOKEN).unwrap_or_else(|_| "\"\"".to_string());
    let proxy_js =
        serde_json::to_string(&proxy_endpoint()).unwrap_or_else(|_| "\"/asset\"".to_string());
    let safe_script = DOM_PROXY_SCRIPT.replace("</script>", "<\\/script>");
    format!(
        "<!--__sandbox_head_marker__--><base href=\"{base}\"><script>window.__SANDBOX={{proxy:{proxy_js},token:{token_js}}};</script><script>{safe_script}</script>",
        base = attr_escape(page_url),
    )
}

fn decode_basic_entities(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_img_src() {
        let html = r#"<html><head></head><body><img src="/foo.png"></body></html>"#;
        let out = rewrite_html(html, "https://example.com/page").unwrap();
        assert!(out.contains("/asset?u="));
        assert!(out.contains("127.0.0.1:8765"));
        assert!(out.contains("example.com%2Ffoo.png"));
    }

    #[test]
    fn rewrites_link_href() {
        let html = r#"<html><head><link rel="stylesheet" href="/a.css"></head><body></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("href=\"http://127.0.0.1:8765/asset?u="));
    }

    #[test]
    fn does_not_rewrite_anchors() {
        let html = r#"<html><head></head><body><a href="/page2">x</a></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains(r#"<a href="/page2">"#));
    }

    #[test]
    fn rewrites_srcset() {
        let html =
            r#"<html><head></head><body><img srcset="/a.png 1x, /b.png 2x"></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("a.png"));
        assert!(out.contains("b.png"));
        assert!(out.contains("/asset?u="));
        assert!(out.contains(" 1x"));
    }

    #[test]
    fn injects_base_and_proxy_script() {
        let html = r#"<html><head><title>x</title></head><body></body></html>"#;
        let out = rewrite_html(html, "https://example.com/p").unwrap();
        assert!(out.contains(r#"<base href="https://example.com/p">"#));
        assert!(out.contains("window.__SANDBOX"));
        assert!(out.contains("sandbox-browser"));
    }

    #[test]
    fn skips_data_urls() {
        let html = r#"<html><head></head><body><img src="data:image/png;base64,AAA"></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("data:image/png"));
    }

    #[test]
    fn rewrites_css_url() {
        let css = r#"body { background: url("/bg.png"); } @import "other.css";"#;
        let out = rewrite_css(css, "https://example.com/styles/main.css");
        assert!(out.contains("http://127.0.0.1:8765/asset?u="));
        assert!(out.contains("example.com%2Fbg.png") || out.contains("example.com%2Fstyles%2F..%2Fbg.png"));
        assert!(out.contains("@import \"http://127.0.0.1:8765/asset?u="));
    }

    #[test]
    fn rewrites_use_href_keeping_fragment() {
        let html = r#"<html><head></head><body><svg><use href="https://cdn.example.com/sprite.svg#icon-x"/></svg></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("/asset?u="));
        assert!(out.contains("cdn.example.com%2Fsprite.svg"));
        assert!(out.contains("#icon-x"));
        // El fragmento NO debe estar dentro del valor encoded de u=.
        assert!(!out.contains("%23icon-x"));
    }

    #[test]
    fn rewrites_inline_style_attribute_url() {
        let html = r#"<html><head></head><body><div style="background: url('/bg.png')"></div></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("http://127.0.0.1:8765/asset?u="));
        assert!(out.contains("example.com%2Fbg.png"));
    }

    #[test]
    fn rewrites_inline_style_block_url() {
        let html = r#"<html><head><style>.x{background:url(/bg.png)}</style></head><body></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("http://127.0.0.1:8765/asset?u="));
        assert!(out.contains("example.com%2Fbg.png"));
    }

    #[test]
    fn strips_page_scripts_keeps_proxy_shim() {
        let html = r#"<html><head><script>alert(1)</script></head><body><script src="/foo.js"></script><p>x</p></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(!out.contains("alert(1)"), "page inline script must be stripped");
        assert!(!out.contains("foo.js"), "page external script must be stripped");
        assert!(out.contains("__SANDBOX"), "our injected shim must survive");
        assert!(out.contains("sandbox-browser"), "our event proxy must survive");
        assert!(out.contains("<p>x</p>"), "non-script content survives");
    }

    #[test]
    fn rewrites_legacy_referrer_meta() {
        let html = r#"<html><head><meta name="referrer" content="origin-when-crossorigin"></head><body></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains(r#"content="origin-when-cross-origin""#));
        assert!(!out.contains("origin-when-crossorigin\""));
    }

    #[test]
    fn rewrites_legacy_referrerpolicy_attribute() {
        let html = r#"<html><head></head><body><a referrerpolicy="origin-when-crossorigin" href="x">x</a></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains(r#"referrerpolicy="origin-when-cross-origin""#));
    }

    #[test]
    fn rewrites_object_data() {
        let html = r#"<html><head></head><body><object data="/embed.pdf"></object></body></html>"#;
        let out = rewrite_html(html, "https://example.com/").unwrap();
        assert!(out.contains("http://127.0.0.1:8765/asset?u="));
        assert!(out.contains("example.com%2Fembed.pdf"));
    }

    #[test]
    fn strips_css_sourcemap_comment() {
        let css = "body{color:red}\n/*# sourceMappingURL=foo.css.map */\n";
        let out = rewrite_css(css, "https://example.com/foo.css");
        assert!(!out.contains("sourceMappingURL"));
        assert!(out.contains("color:red"));
    }
}
