use worker::*;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();
    router
        .get("/", |_, _| Response::ok("Hello, World from Rust!"))
        .run(req, env)
        .await
}