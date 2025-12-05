use scraper::{Html, Selector};
use serde::Serialize;
use worker::{Fetch, Request, RequestInit};

pub async fn get_html(url: impl AsRef<str>) -> Result<String, anyhow::Error> {
    let cache = worker::Cache::default();
    let url = url.as_ref();

    let mut res = if let Some(cached) = cache.get(url, false).await? {
        tracing::trace!("Cache HIT for {url}");
        cached
    } else {
        let req = Request::new_with_init(url, &RequestInit::new())?;
        let mut res = Fetch::Request(req).send().await?;
        let mut cloned_res = res.cloned()?;

        cloned_res
            .headers_mut()
            .set("Cache-Control", "max-age=60")?; // cache for 60 seconds
        cache.put(url, cloned_res.cloned()?).await?;

        res
    };

    Ok(res.text().await?)
}

#[derive(Debug, Serialize)]
pub struct ScrapeResult {
    title: String,
    img: String,
    href: String,
}

pub fn parse_html(html: impl AsRef<str>) -> anyhow::Result<Vec<ScrapeResult>> {
    let html = Html::parse_document(html.as_ref());
    let get_selector = |s: &str| {
        Selector::parse(s).map_err(|x| anyhow::anyhow!("Failed parsing CSS Selector ({s}): {x}"))
    };
    let urlcleaner = clearurls::UrlCleaner::from_embedded_rules()?;

    let article_s = get_selector("div .post-container > article")?;
    let header_s = get_selector("header > h1 > a")?;
    let img_s = get_selector("div > a > img")?;
    let link_s = get_selector("p > a")?;

    Ok(html
        .select(&article_s)
        .map(|post| {
            let title = post
                .select(&header_s)
                .next()
                .map(|x| x.text().collect::<Vec<_>>().join(" "))
                .unwrap_or(String::from(""));

            let img = post
                .select(&img_s)
                .next()
                .and_then(|x| x.attr("src"))
                .map(|x| x.to_string())
                .unwrap_or(String::from(""));

            let img = if let Ok(mut img_url) = url::Url::parse(&img) {
                img_url.set_query(None);
                img_url.to_string()
            } else {
                img
            };

            let links = post.select(&link_s).collect::<Vec<_>>();

            let href = links
                .iter()
                .find(|link| {
                    link.text()
                        .collect::<String>()
                        .to_uppercase()
                        .starts_with("VOLUME ")
                })
                .or_else(|| {
                    links.iter().find(|link| {
                        link.attr("href")
                            .unwrap_or("#####MISSINGLINK#####")
                            .to_lowercase()
                            .contains("#more-")
                    })
                })
                .and_then(|link| link.attr("href"))
                .map(|x| x.to_string())
                .unwrap_or(String::from(""));

            let mut href_url = url::Url::parse(&href).expect("Failed parsing href");
            href_url.set_fragment(None);

            let href = urlcleaner
                .clear_single_url(&href_url)
                .expect("Failed sanitizing url")
                .to_string();

            ScrapeResult { title, img, href }
        })
        .collect())
}
