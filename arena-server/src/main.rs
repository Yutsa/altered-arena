//! Altered Arena game server.
//!
//! Protocol v1 (protocol/protocol.ts) over HTTP, games in memory (persisted to DATA_DIR as JSON,
//! reloaded at start), bot opponents, matchmaking queue, and the web client (CLIENT_DIR) with the
//! card art (ART_DIR) as static files.
//!
//! Environment: PORT (8080), CLIENT_DIR (client/dist), ART_DIR (../AlteredBGA/img/illustrations),
//! PROTOCOL_DIR (protocol/data), DATA_DIR (data).

use altered_engine::model::Seat;
use altered_engine::{Answer, Game, PlayerConfig};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Notify, RwLock};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

struct GameEntry {
    game: Mutex<Game>,
    changed: Notify,
}

#[derive(Clone)]
struct Ticket {
    name: String,
    deck: Option<String>,
    /// Set when matched: (game id, player id, player token)
    matched: Option<(String, u32, String)>,
    created: u64,
}

struct AppState {
    games: RwLock<HashMap<String, Arc<GameEntry>>>,
    dirty: Mutex<HashSet<String>>,
    queue: Mutex<HashMap<String, Ticket>>,
    queue_changed: Notify,
    next_id: AtomicU64,
    cards_json: Vec<u8>,
    decks_json: Vec<u8>,
    data_dir: std::path::PathBuf,
}

type Shared = Arc<AppState>;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(42)
}

/// 128-bit secret from the OS random source
fn new_token() -> String {
    use std::io::Read;
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    }
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

fn json_bytes(body: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

fn error(status: StatusCode, msg: impl Into<String>, user: bool) -> Response {
    (status, Json(json!({"error": msg.into(), "user": user}))).into_response()
}

#[tokio::main]
async fn main() {
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let port = env("PORT", "8080");
    let client_dir = env("CLIENT_DIR", "client/dist");
    let art_dir = env("ART_DIR", "../AlteredBGA/img/illustrations");
    let protocol_dir = env("PROTOCOL_DIR", "protocol/data");
    let data_dir = std::path::PathBuf::from(env("DATA_DIR", "data"));
    let _ = std::fs::create_dir_all(data_dir.join("games"));

    let state = Arc::new(AppState {
        games: RwLock::new(HashMap::new()),
        dirty: Mutex::new(HashSet::new()),
        queue: Mutex::new(HashMap::new()),
        queue_changed: Notify::new(),
        next_id: AtomicU64::new(1),
        cards_json: std::fs::read(format!("{}/cards.json", protocol_dir)).expect("protocol/data/cards.json"),
        decks_json: std::fs::read(format!("{}/decks.json", protocol_dir)).expect("protocol/data/decks.json"),
        data_dir: data_dir.clone(),
    });
    load_games(&state).await;
    altered_engine::catalog::catalog();

    // Persist changed games every few seconds
    let s2 = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            save_dirty(&s2).await;
        }
    });

    let index = format!("{}/index.html", client_dir);
    let app = Router::new()
        .route("/api/health", get(|| async { Json(json!({"ok": true})) }))
        .route("/api/v1/cards", get(cards))
        .route("/api/v1/decks", get(decks))
        .route("/api/v1/games", post(create_game))
        .route("/api/v1/games/{id}", get(view_game))
        .route("/api/v1/games/{id}/answer", post(answer))
        .route("/api/v1/matchmaking", post(join_queue))
        .route("/api/v1/matchmaking/{ticket}", get(poll_queue).delete(leave_queue))
        .nest_service("/assets/cards", ServeDir::new(art_dir))
        .fallback_service(ServeDir::new(&client_dir).fallback(ServeFile::new(index)))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // Differential tests only: full hidden state and arbitrary positions
    let app = if std::env::var("ARENA_DEBUG").is_ok() {
        println!("Debug routes enabled (ARENA_DEBUG)");
        app.merge(
            Router::new()
                .route("/api/debug/games", post(create_from_snapshot))
                .route("/api/debug/games/{id}/state", get(debug_state))
                .with_state(state.clone()),
        )
    } else {
        app
    };
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.expect("bind");
    println!("Altered Arena server on http://localhost:{}", port);
    let s3 = state.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            save_dirty(&s3).await;
        })
        .await
        .unwrap();
}

async fn cards(State(s): State<Shared>) -> Response {
    ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "public, max-age=3600")], s.cards_json.clone()).into_response()
}

async fn decks(State(s): State<Shared>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], s.decks_json.clone()).into_response()
}

// ---------------------------------------------------------------------------
// Games
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NewPlayer {
    name: Option<String>,
    #[serde(default)]
    bot: bool,
    deck: Option<String>,
}

#[derive(Deserialize)]
struct NewGame {
    players: Vec<NewPlayer>,
    seed: Option<u64>,
}

fn clean_name(n: Option<&str>, i: usize) -> String {
    let n: String = n.unwrap_or("").chars().filter(|c| c.is_alphanumeric() || " _-".contains(*c)).take(24).collect();
    if n.trim().is_empty() {
        format!("Player {}", i + 1)
    } else {
        n
    }
}

/// Registers a game; each human seat gets a secret token, returned only to whoever created
/// the game (lobby: the same person may play both seats) or to the matched player
async fn insert_game(s: &Shared, mut game: Game) -> Value {
    game.run_bots();
    for p in game.players.iter_mut() {
        if !p.bot && p.token.is_empty() {
            p.token = new_token();
        }
    }
    let id = game.id.clone();
    let players: Vec<Value> = game.players.iter().map(|p| {
        let mut v = json!({"id": p.id, "name": p.name, "bot": p.bot});
        if !p.bot {
            v["token"] = json!(p.token);
        }
        v
    }).collect();
    s.games.write().await.insert(id.clone(), Arc::new(GameEntry { game: Mutex::new(game), changed: Notify::new() }));
    s.dirty.lock().await.insert(id.clone());
    json!({"gameId": id, "players": players})
}

/// Short unguessable-enough id: counter + time bits, base 36
fn new_game_id(s: &Shared) -> String {
    let n = s.next_id.fetch_add(1, Ordering::Relaxed);
    let mut x = (seed() & 0xFFFF_FFFF) ^ (n << 32);
    let mut out = String::new();
    while x > 0 {
        out.push(std::char::from_digit((x % 36) as u32, 36).unwrap());
        x /= 36;
    }
    out
}

async fn create_game(State(s): State<Shared>, Json(req): Json<NewGame>) -> Response {
    if req.players.len() != 2 {
        return error(StatusCode::BAD_REQUEST, "A game needs 2 players", true);
    }
    let decks = altered_engine::catalog::catalog();
    for p in &req.players {
        if let Some(d) = &p.deck {
            if decks.starter(d).is_none() {
                return error(StatusCode::BAD_REQUEST, format!("Unknown deck {}", d), true);
            }
        }
    }
    let cfg = |i: usize| {
        let p = &req.players[i];
        PlayerConfig { id: 2001 + i as u32, name: clean_name(p.name.as_deref(), i), bot: p.bot, deck: p.deck.clone() }
    };
    let id = new_game_id(&s);
    let game = Game::new(id, [cfg(0), cfg(1)], req.seed.unwrap_or_else(seed));
    Json(insert_game(&s, game).await).into_response()
}

#[derive(Deserialize)]
struct ViewQuery {
    player: Option<u32>,
    since: Option<u64>,
    token: Option<String>,
}

/// Seat of the requester: the token decides (a wrong player/token pair is refused)
fn seat_of(g: &Game, q: &ViewQuery, headers: &axum::http::HeaderMap) -> Result<Option<Seat>, Response> {
    let token = q.token.clone().or_else(|| {
        headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")).map(String::from)
    });
    let Some(token) = token.filter(|t| !t.is_empty()) else { return Ok(None) };
    match g.players.iter().position(|p| !p.token.is_empty() && p.token == token) {
        Some(seat) if q.player.map(|id| g.players[seat].id == id).unwrap_or(true) => Ok(Some(seat)),
        _ => Err(error(StatusCode::FORBIDDEN, "Wrong token", false)),
    }
}

async fn entry(s: &Shared, id: &str) -> Option<Arc<GameEntry>> {
    s.games.read().await.get(id).cloned()
}

async fn view_game(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<ViewQuery>, headers: axum::http::HeaderMap) -> Response {
    let Some(e) = entry(&s, &id).await else { return error(StatusCode::NOT_FOUND, "Unknown game", false) };
    if let Some(since) = q.since {
        // Long polling: wait for a change (or 20 s)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let notified = e.changed.notified();
            if e.game.lock().await.seq > since {
                break;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                break;
            }
        }
    }
    let g = e.game.lock().await;
    // Without a token: spectator view (no hidden cards, no decision)
    match seat_of(&g, &q, &headers) {
        Ok(seat) => json_bytes(g.view_json(seat)),
        Err(r) => r,
    }
}

async fn answer(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<ViewQuery>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let Some(e) = entry(&s, &id).await else { return error(StatusCode::NOT_FOUND, "Unknown game", false) };
    let mut g = e.game.lock().await;
    let seat = match seat_of(&g, &q, &headers) {
        Ok(Some(seat)) => seat,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "A player token is required", false),
        Err(r) => return r,
    };
    let decision_id = body.get("decisionId").and_then(Value::as_str).unwrap_or("");
    let is_undo = body.get("type").and_then(Value::as_str) == Some("undo");
    if !is_undo && g.decision_id(seat).as_deref() != Some(decision_id) {
        return error(StatusCode::CONFLICT, "This decision is no longer current", true);
    }
    let answer: Answer = match serde_json::from_value(body.clone()) {
        Ok(a) => a,
        Err(err) => return error(StatusCode::BAD_REQUEST, format!("Bad answer: {}", err), true),
    };
    if let Err(err) = g.answer(seat as Seat, answer) {
        return error(StatusCode::BAD_REQUEST, err.0, true);
    }
    g.run_bots();
    let view = g.view_json(Some(seat));
    drop(g);
    e.changed.notify_waiters();
    s.dirty.lock().await.insert(id);
    json_bytes(view)
}

// ---------------------------------------------------------------------------
// Matchmaking: first come, first served; a ticket waits until another player joins
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JoinQueue {
    name: Option<String>,
    deck: Option<String>,
}

async fn join_queue(State(s): State<Shared>, Json(req): Json<JoinQueue>) -> Response {
    let ticket_id = format!("t{}", seed());
    let name = clean_name(req.name.as_deref(), 0);
    let mut queue = s.queue.lock().await;
    queue.retain(|_, t| t.matched.is_some() || now() - t.created < 120);
    let waiting = queue.iter().find(|(_, t)| t.matched.is_none()).map(|(k, t)| (k.clone(), t.clone()));
    match waiting {
        Some((other_id, other)) => {
            let id = new_game_id(&s);
            let game = Game::new(
                id.clone(),
                [
                    PlayerConfig { id: 2001, name: other.name.clone(), bot: false, deck: other.deck.clone() },
                    PlayerConfig { id: 2002, name: name.clone(), bot: false, deck: req.deck.clone() },
                ],
                seed(),
            );
            let created = insert_game(&s, game).await;
            let token = |i: usize| created["players"][i]["token"].as_str().unwrap_or("").to_string();
            queue.get_mut(&other_id).unwrap().matched = Some((id.clone(), 2001, token(0)));
            queue.insert(ticket_id.clone(), Ticket { name, deck: req.deck, matched: Some((id.clone(), 2002, token(1))), created: now() });
            drop(queue);
            s.queue_changed.notify_waiters();
            Json(json!({"ticket": ticket_id, "status": "matched", "gameId": id, "player": 2002, "token": token(1)})).into_response()
        }
        None => {
            queue.insert(ticket_id.clone(), Ticket { name, deck: req.deck, matched: None, created: now() });
            Json(json!({"ticket": ticket_id, "status": "waiting"})).into_response()
        }
    }
}

async fn poll_queue(State(s): State<Shared>, Path(ticket): Path<String>) -> Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let notified = s.queue_changed.notified();
        {
            let mut queue = s.queue.lock().await;
            let Some(t) = queue.get_mut(&ticket) else { return error(StatusCode::NOT_FOUND, "Unknown ticket", true) };
            t.created = t.created.max(now().saturating_sub(60));
            if let Some((g, p, tok)) = &t.matched {
                return Json(json!({"ticket": ticket, "status": "matched", "gameId": g, "player": p, "token": tok})).into_response();
            }
        }
        if tokio::time::timeout_at(deadline, notified).await.is_err() {
            return Json(json!({"ticket": ticket, "status": "waiting"})).into_response();
        }
    }
}

async fn leave_queue(State(s): State<Shared>, Path(ticket): Path<String>) -> Response {
    s.queue.lock().await.remove(&ticket);
    Json(json!({"ok": true})).into_response()
}

// ---------------------------------------------------------------------------
// Debug (differential tests against the original engine)
// ---------------------------------------------------------------------------

async fn create_from_snapshot(State(s): State<Shared>, Json(snapshot): Json<Value>) -> Response {
    match Game::from_snapshot(new_game_id(&s), &snapshot) {
        Ok(game) => Json(insert_game(&s, game).await).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e.0, true),
    }
}

async fn debug_state(State(s): State<Shared>, Path(id): Path<String>) -> Response {
    let Some(e) = entry(&s, &id).await else { return error(StatusCode::NOT_FOUND, "Unknown game", false) };
    let g = e.game.lock().await;
    Json(g.snapshot()).into_response()
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn save_dirty(s: &Shared) {
    let ids: Vec<String> = s.dirty.lock().await.drain().collect();
    for id in ids {
        let Some(e) = entry(s, &id).await else { continue };
        let json = serde_json::to_vec(&*e.game.lock().await).unwrap_or_default();
        let path = s.data_dir.join("games").join(format!("{}.json", id));
        let tmp = path.with_extension("tmp");
        if tokio::fs::write(&tmp, json).await.is_ok() {
            let _ = tokio::fs::rename(&tmp, &path).await;
        }
    }
}

async fn load_games(s: &Shared) {
    let Ok(mut dir) = tokio::fs::read_dir(s.data_dir.join("games")).await else { return };
    let mut n = 0;
    let mut max_id = 0;
    while let Ok(Some(f)) = dir.next_entry().await {
        let Ok(bytes) = tokio::fs::read(f.path()).await else { continue };
        let Ok(game) = serde_json::from_slice::<Game>(&bytes) else { continue };
        max_id += 1;
        s.games.write().await.insert(game.id.clone(), Arc::new(GameEntry { game: Mutex::new(game), changed: Notify::new() }));
        n += 1;
    }
    s.next_id.store(max_id.max(1), Ordering::Relaxed);
    if n > 0 {
        println!("{} games loaded", n);
    }
}
