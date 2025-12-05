use futures::SinkExt;
use worker::*;

mod fetcher;

fn get_envvar(env: &Env) -> worker::wasm_bindgen::JsValue {
    env.var("ENV")
        .unwrap_or(worker::Var::from(worker::wasm_bindgen::JsValue::from_str(
            "production",
        )))
        .as_ref()
        .clone()
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    tracing_worker::init_tracing(if get_envvar(&env) == "production" {
        tracing::Level::INFO
    } else {
        tracing::Level::TRACE
    });

    let cache = Cache::default();
    if let Some(cached) = cache.get(&req, false).await? {
        tracing::trace!("Cache HIT");
        return Ok(cached);
    }

    let max_page = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "maxpage")
        .map(|(_, v)| v.parse::<usize>().unwrap_or(1))
        .unwrap_or(1)
        .min(10);

    let mut res = Router::new()
        .get("/", |_, _| Response::error("", 404))
        .get_async("/stream", |_req, _ctx| async move {
            tracing::trace!("Fetching {max_page} pages");

            let (mut tx, rx) = futures::channel::mpsc::channel(4);

            wasm_bindgen_futures::spawn_local(async move {
                let res = async {
                    for x in std::iter::once(String::from(""))
                        .chain((1..max_page).map(|x| format!("page/{x}/")))
                        .map(|x: String| format!("https://jnovels.com/{x}?s=epub"))
                    {
                        for l in fetcher::parse_html(fetcher::get_html(&x).await?)? {
                            let asdf = serde_json::to_string(&l)? + "\n";
                            tx.send(Ok::<_, &str>(asdf.into_bytes())).await?;
                        }
                    }

                    tx.close_channel();

                    anyhow::Ok(())
                }
                .await;

                if let Err(e) = res {
                    tracing::error!(?e, "Failed running task");
                }
            });

            Response::from_stream(rx)
        })
        .get_async("/get", |_req, _ctx| async move {
            tracing::trace!("Fetching {max_page} pages");

            let fetched = std::iter::once(String::from(""))
                .chain((1..max_page).map(|x| format!("page/{x}/")))
                .map(|x: String| format!("https://jnovels.com/{x}?s=epub"))
                .map(fetcher::get_html);

            let mut iter_err = anyhow::Ok(());

            let res = futures::future::try_join_all(fetched)
                .await
                .expect("Failed to get HTML")
                .iter()
                .map(fetcher::parse_html)
                // Source - https://stackoverflow.com/a/63120052
                // Posted by user4815162342, modified by community. See post `'Timeline' for change history
                // Retrieved 2025-12-06, License - CC BY-SA 4.0`
                // A nifty way to escape on err while iter, which stops it if it do.
                // A way to avoid allocs
                .scan((), |_, item| item.map_err(|e| iter_err = Err(e)).ok())
                .flatten()
                .collect::<Vec<_>>();

            if let Err(e) = iter_err {
                return Response::error(format!("Failed parsing HTML: {e}"), 400);
            }

            Response::from_json(&res)
        })
        .run(req.clone().expect("Failed to clone request"), env)
        .await?;

    res.headers_mut().set("Cache-Control", "max-age=120")?;
    if let Ok(res) = res.cloned() {
        cache.put(&req, res).await?;
    }

    Ok(res)
}
