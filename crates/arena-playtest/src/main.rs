mod session;

use std::net::Ipv4Addr;
use std::time::Duration;

use arena_core::TICKS_PER_SECOND;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::header;
use axum::response::{Html, Response};
use axum::routing::get;
use futures_util::SinkExt;
use serde_json::Value;
use session::Session;
use tokio::net::TcpListener;
use tokio::time::{Instant, MissedTickBehavior, interval_at, timeout};

#[derive(Clone, Copy)]
struct Options {
    port: u16,
    seed: Option<u64>,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            port: 8080,
            seed: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--port" => options.port = parse_value(&arg, args.next())?,
                "--seed" => options.seed = Some(parse_value(&arg, args.next())?),
                _ => {
                    return Err(format!(
                        "Unknown argument: {arg}. Usage: arena-playtest [--port PORT] [--seed SEED]"
                    ));
                }
            }
        }
        Ok(options)
    }
}

fn parse_value<T: std::str::FromStr>(name: &str, value: Option<String>) -> Result<T, String> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("Invalid value for {name}"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse()?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, options.port)).await?;
    let router = Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/style.css",
            get(|| async { asset("text/css", include_str!("../web/style.css")) }),
        )
        .route(
            "/app.js",
            get(|| async { asset("text/javascript", include_str!("../web/app.js")) }),
        )
        .route(
            "/canvas.js",
            get(|| async { asset("text/javascript", include_str!("../web/canvas.js")) }),
        )
        .route("/ws", get(move |ws| upgrade(ws, options.seed)));
    println!("http://{}", listener.local_addr()?);
    axum::serve(listener, router).await?;
    Ok(())
}

fn asset(
    content_type: &'static str,
    body: &'static str,
) -> ([(header::HeaderName, &'static str); 1], &'static str) {
    ([(header::CONTENT_TYPE, content_type)], body)
}

async fn upgrade(ws: WebSocketUpgrade, seed: Option<u64>) -> Response {
    ws.max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| play(socket, seed.unwrap_or_else(rand::random)))
}

async fn play(mut socket: WebSocket, seed: u64) {
    let mut session = Session::new(seed);
    if !send(&mut socket, session.state()).await {
        return;
    }
    let period = Duration::from_secs_f64(1.0 / f64::from(TICKS_PER_SECOND));
    let mut ticks = interval_at(Instant::now() + period, period);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        let reply = tokio::select! {
            _ = ticks.tick() => Some(session.tick()),
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => session.receive(text.as_str()),
                Some(Ok(Message::Close(_))) => {
                    // Tungstenite has queued a matching close frame.
                    let _ = timeout(Duration::from_secs(2), socket.flush()).await;
                    return;
                }
                Some(Err(_)) | None => return,
                Some(Ok(_)) => None,
            },
        };
        if let Some(reply) = reply
            && !send(&mut socket, reply).await
        {
            return;
        }
    }
}

async fn send(socket: &mut WebSocket, reply: Value) -> bool {
    let message = Message::Text(reply.to_string().into());
    matches!(
        timeout(Duration::from_secs(2), socket.send(message)).await,
        Ok(Ok(()))
    )
}
