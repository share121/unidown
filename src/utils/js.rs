use anyhow::anyhow;
use base64::{Engine, engine::general_purpose};
use crossfire::{
    MTx, mpsc,
    oneshot::{self, TxOneshot},
};
use reqwest::{Client, Url};
use rquickjs::{Context, Function, Object, Runtime};

struct JsRuntimeInner {
    _runtime: Runtime,
    context: Context,
}

impl JsRuntimeInner {
    fn new() -> anyhow::Result<Self> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let source_code = include_str!("../../js/bundle.js");
        context.with(|ctx| {
            let globals = ctx.globals();
            globals.set(
                "atob",
                Function::new(ctx.clone(), |s: String| -> String {
                    let cleaned = s.replace([' ', '\n', '\r', '\t'], "");
                    match general_purpose::STANDARD.decode(cleaned) {
                        Ok(bytes) => bytes.iter().map(|&b| b as char).collect(),
                        Err(_) => "".to_string(), // 或者抛出 JS 异常
                    }
                })?,
            )?;
            ctx.eval::<(), _>(source_code)
                .map_err(|e| anyhow!("JS Eval Error: {:?}", e))
        })?;
        Ok(Self {
            _runtime: runtime,
            context,
        })
    }

    fn douyin_gen_req(&self, url: &str) -> anyhow::Result<(String, String)> {
        self.context.with(|ctx| {
            let douyin_obj: Object = ctx.globals().get("Douyin")?;
            let gen_req: Function = douyin_obj.get("genReq")?;
            let req_info: Object = gen_req.call((url,)).map_err(|_| {
                let exc = ctx.catch();
                if let Some(e) = exc.as_exception() {
                    let msg = e.message().unwrap_or_default();
                    let stack = e.stack().unwrap_or_default();
                    anyhow!("JS Runtime Error: {}\nStack: {}", msg, stack)
                } else {
                    anyhow!("JS Unknown Exception")
                }
            })?;
            let body: String = req_info
                .get("body")
                .map_err(|_| anyhow!("Missing or invalid 'body'"))?;
            let auth: String = req_info
                .get("auth")
                .map_err(|_| anyhow!("Missing or invalid 'auth'"))?;
            Ok((body, auth))
        })
    }

    fn douyin_gen_output(&self, resp_text: &str) -> anyhow::Result<(String, Url)> {
        self.context.with(|ctx| {
            let douyin_obj: Object = ctx.globals().get("Douyin")?;
            let gen_output: Function = douyin_obj.get("genOuput")?;
            let result: Object = gen_output.call((resp_text,)).map_err(|_| {
                let exc = ctx.catch();
                if let Some(e) = exc.as_exception() {
                    let msg = e.message().unwrap_or_default();
                    let stack = e.stack().unwrap_or_default();
                    anyhow!("JS Runtime Error: {}\nStack: {}", msg, stack)
                } else {
                    anyhow!("JS Unknown Exception")
                }
            })?;
            let title: String = result
                .get("title")
                .map_err(|_| anyhow!("Missing or invalid 'title'"))?;
            let url: String = result
                .get("url")
                .map_err(|_| anyhow!("Missing or invalid 'url'"))?;
            let url = url.parse()?;
            Ok((title, url))
        })
    }
}

#[derive(Clone)]
pub struct JsRuntime {
    tx: MTx<mpsc::List<Action>>,
}

enum Action {
    /// 返回 (body, auth_token)
    DouyinGenReq(String, TxOneshot<anyhow::Result<(String, String)>>),
    /// 返回 (title, video_url)
    DouyinGenOutput(String, TxOneshot<anyhow::Result<(String, Url)>>),
}

impl Default for JsRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl JsRuntime {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_blocking();
        std::thread::spawn(move || {
            let runtime = JsRuntimeInner::new().expect("无法启动 JavaScript 引擎");
            while let Ok(msg) = rx.recv() {
                match msg {
                    Action::DouyinGenReq(url, tx) => tx.send(runtime.douyin_gen_req(&url)),
                    Action::DouyinGenOutput(resp_text, tx) => {
                        tx.send(runtime.douyin_gen_output(&resp_text))
                    }
                }
            }
        });
        Self { tx }
    }

    async fn douyin_gen_req(&self, url: String) -> anyhow::Result<(String, String)> {
        let (tx, rx) = oneshot::oneshot();
        self.tx.send(Action::DouyinGenReq(url, tx))?;
        rx.await?
    }

    async fn douyin_gen_output(&self, resp_text: String) -> anyhow::Result<(String, Url)> {
        let (tx, rx) = oneshot::oneshot();
        self.tx.send(Action::DouyinGenOutput(resp_text, tx))?;
        rx.await?
    }

    pub async fn parse_douyin(&self, url: String, client: Client) -> anyhow::Result<(String, Url)> {
        let (body, auth) = self.douyin_gen_req(url).await?;
        let resp_text = client
            .post("https://www.hellotik.app/api/parse")
            .header("Content-Type", "application/json")
            .header("X-Auth-Token", auth)
            .body(body)
            .send()
            .await?
            .text()
            .await?;
        self.douyin_gen_output(resp_text).await
    }
}
