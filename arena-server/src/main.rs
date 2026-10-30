//! Altered Arena game server.
//!
//! Protocol v1 (protocol/protocol.ts) over HTTP, games in memory (saved to DATA_DIR as JSON every
//! PERSIST_SECS, unloaded when idle and reloaded on demand), bot opponents, matchmaking queue, and
//! the web client (CLIENT_DIR) with the card art (ART_DIR) as static files.
//!
//! Environment: PORT (8080), CLIENT_DIR (client/dist), ART_DIR (../AlteredBGA/img/illustrations),
//! PROTOCOL_DIR (protocol/data), DATA_DIR (data), PERSIST_SECS (10), ARENA_DEBUG (debug routes),
//! ENGINE_THREADS (number of threads running the rules engine `rules/`, default: CPUs),
//! ENGINE_STACK_MB (their stack size, 256), IMITATION_DIR, AI_PARAMS_DIR and TOURNAMENT_DECKS (the
//! AIs a bot seat can play, see `ais`).
//! Recommended: MIMALLOC_PURGE_DELAY=0 (returns freed memory to the OS sooner).

/// Faster than the system allocator for many small allocations, and less fragmentation
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod ais;
mod engines;
mod replays;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use altered_ai::play::{Agent, Observation, SimpleBot};
use engines::{EnginePool, NewSeat, ServerGame};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Notify, RwLock};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

struct GameEntry {
    game: Arc<Mutex<ServerGame>>,
    changed: Notify,
    /// Last request on this game (unix seconds), to unload idle games from memory
    last_access: AtomicU64,
    /// The game has AI seats (`kick_ais`)
    has_ai: bool,
    ai: Arc<std::sync::Mutex<AiPlayers>>,
    /// A task plays the AI seats' decisions
    ai_running: AtomicBool,
    /// Counts the requests to play them
    ai_kicks: AtomicU64,
}

impl GameEntry {
    fn new(game: ServerGame) -> Arc<GameEntry> {
        Arc::new(GameEntry {
            has_ai: !game.ai_seats().is_empty(),
            game: Arc::new(Mutex::new(game)),
            changed: Notify::new(),
            last_access: AtomicU64::new(now()),
            ai: Default::default(),
            ai_running: AtomicBool::new(false),
            ai_kicks: AtomicU64::new(0),
        })
    }
}

/// The AI players of a game, built at their first decision (again after a reload)
#[derive(Default)]
struct AiPlayers {
    agents: HashMap<u32, Box<dyn Agent>>,
    /// Each seat's decklist (an AI's own; a human's is not read)
    decks: [Value; 2],
    rng: Option<ChaCha8Rng>,
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
    pool: EnginePool,
    games: RwLock<HashMap<String, Arc<GameEntry>>>,
    dirty: Mutex<HashSet<String>>,
    queue: Mutex<HashMap<String, Ticket>>,
    queue_changed: Notify,
    next_id: AtomicU64,
    cards_json: Vec<u8>,
    decks_json: Vec<u8>,
    data_dir: std::path::PathBuf,
    ais: ais::AiDirs,
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
    let threads: usize = env("ENGINE_THREADS", "0").parse().ok().filter(|n| *n > 0).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));
    let stack_mb: usize = env("ENGINE_STACK_MB", "256").parse().unwrap_or(256);

    let state = Arc::new(AppState {
        pool: EnginePool::new(threads, stack_mb),
        games: RwLock::new(HashMap::new()),
        dirty: Mutex::new(HashSet::new()),
        queue: Mutex::new(HashMap::new()),
        queue_changed: Notify::new(),
        next_id: AtomicU64::new(1),
        cards_json: std::fs::read(format!("{}/cards.json", protocol_dir)).expect("protocol/data/cards.json"),
        decks_json: std::fs::read(format!("{}/decks.json", protocol_dir)).expect("protocol/data/decks.json"),
        data_dir: data_dir.clone(),
        ais: ais::AiDirs::from_env(),
    });
    altered_rules::managers::cards::classes();

    // Persist changed games every few seconds (a crash loses at most that much play)
    let persist_secs: u64 = env("PERSIST_SECS", "10").parse().unwrap_or(10);
    let s2 = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(persist_secs)).await;
            save_dirty(&s2).await;
            unload_idle(&s2).await;
        }
    });

    let index = format!("{}/index.html", client_dir);
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/v1/cards", get(cards))
        .route("/api/v1/decks", get(decks))
        .route("/api/v1/ais", get(list_ais))
        .route("/api/v1/games", post(create_game))
        .route("/api/v1/games/{id}", get(view_game))
        .route("/api/v1/games/{id}/answer", post(answer))
        .route("/api/v1/matchmaking", post(join_queue))
        .route("/api/v1/matchmaking/{ticket}", get(poll_queue).delete(leave_queue))
        .route("/api/v1/replays", get(list_replays))
        .route("/api/v1/replays/{table}", get(view_replay))
        .nest_service("/assets/cards", ServeDir::new(art_dir))
        .fallback_service(ServeDir::new(&client_dir).fallback(ServeFile::new(index)))
        // Views are small (a few KB): the fastest level keeps most of the gain for a fraction of the CPU
        .layer(CompressionLayer::new().quality(tower_http::CompressionLevel::Fastest))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // Debugging only: full hidden state
    let app = if std::env::var("ARENA_DEBUG").is_ok() {
        println!("Debug routes enabled (ARENA_DEBUG)");
        app.merge(
            Router::new()
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

async fn health() -> Response {
    Json(json!({"ok": true, "engine": "rules"})).into_response()
}

async fn cards(State(s): State<Shared>) -> Response {
    ([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "public, max-age=3600")], s.cards_json.clone()).into_response()
}

async fn list_replays(State(s): State<Shared>) -> Response {
    match s.pool.run(replays::list).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e, false),
    }
}

#[derive(Deserialize)]
struct ReplayQuery {
    /// 2001 or 2002 (that player's hidden cards are shown); none: spectator
    seat: Option<u32>,
}

async fn view_replay(State(s): State<Shared>, Path(table): Path<String>, Query(q): Query<ReplayQuery>) -> Response {
    let seat = q.seat.filter(|p| *p == 2001 || *p == 2002);
    match s.pool.run(move || replays::views(&table, seat)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::NOT_FOUND, e, true),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e, false),
    }
}

async fn decks(State(s): State<Shared>) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], s.decks_json.clone()).into_response()
}

/// The AIs a bot seat can play (read from their files at each request)
async fn list_ais(State(s): State<Shared>) -> Response {
    let s2 = s.clone();
    match tokio::task::spawn_blocking(move || s2.ais.list()).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string(), false),
    }
}

// ---------------------------------------------------------------------------
// Engine calls
// ---------------------------------------------------------------------------

/// Runs `op` on the game, on an engine thread (large stack), holding the game's lock. With
/// `backup`, a panic of the `rules` engine leaves the game as it was before the call (views don't
/// need it: the engine builds them on a copy of the game).
async fn with_game<T: Send + 'static>(s: &Shared, e: &Arc<GameEntry>, backup: bool, op: impl FnOnce(&mut ServerGame) -> T + Send + 'static) -> Result<T, String> {
    let mut guard = e.game.clone().lock_owned().await;
    s.pool
        .run(move || {
            let backup = backup.then(|| guard.clone());
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| op(&mut guard))) {
                Ok(v) => Ok(v),
                Err(p) => {
                    if let Some(b) = backup {
                        *guard = b;
                    }
                    Err(p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "engine panic".into()))
                }
            }
        })
        .await?
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
    /// A bot seat: the AI that plays it (`GET /api/v1/ais`), the simple bot by default
    ai: Option<String>,
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

/// Creates a game on an engine thread, lets the bots play, gives each human seat a secret
/// token (returned only to whoever created the game, or to the matched player) and registers it
async fn insert_game(s: &Shared, id: String, players: [NewSeat; 2], seed: u64) -> Result<Value, String> {
    let game = s
        .pool
        .run(move || -> Result<ServerGame, String> {
            let mut game = ServerGame::new(id, players, seed)?;
            game.run_bots();
            game.set_tokens(new_token);
            Ok(game)
        })
        .await??;
    let id = game.id().to_string();
    let players: Vec<Value> = game
        .seats()
        .into_iter()
        .map(|(pid, name, bot, token)| {
            let mut v = json!({"id": pid, "name": name, "bot": bot});
            if !bot {
                v["token"] = json!(token);
            }
            v
        })
        .collect();
    let e = GameEntry::new(game);
    s.games.write().await.insert(id.clone(), e.clone());
    s.dirty.lock().await.insert(id.clone());
    kick_ais(s, &id, &e);
    Ok(json!({"gameId": id, "players": players}))
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

fn known_deck(deck: &str) -> bool {
    altered_rules::v1::deck_summaries().iter().any(|d| d["id"] == json!(deck))
}

async fn create_game(State(s): State<Shared>, Json(req): Json<NewGame>) -> Response {
    if req.players.len() != 2 {
        return error(StatusCode::BAD_REQUEST, "A game needs 2 players", true);
    }
    let mut seats = vec![];
    for (i, p) in req.players.iter().enumerate() {
        if let Some(d) = &p.deck {
            if !known_deck(d) {
                return error(StatusCode::BAD_REQUEST, format!("Unknown deck {}", d), true);
            }
        }
        let mut seat = NewSeat { name: clean_name(p.name.as_deref(), i), bot: p.bot, deck: p.deck.clone(), ai: None };
        // An AI plays its own deck, under its own name
        if let Some(ai) = p.ai.clone().filter(|a| p.bot && a != ais::SIMPLE) {
            let s2 = s.clone();
            let ai2 = ai.clone();
            let Ok(Some((name, deck))) = tokio::task::spawn_blocking(move || s2.ais.describe(&ai2)).await else {
                return error(StatusCode::BAD_REQUEST, format!("Unknown AI {}", ai), true);
            };
            seat = NewSeat { name, bot: true, deck: None, ai: Some((ai, deck)) };
        }
        seats.push(seat);
    }
    let seats: [NewSeat; 2] = seats.try_into().unwrap_or_else(|_| unreachable!());
    let id = new_game_id(&s);
    match insert_game(&s, id, seats, req.seed.unwrap_or_else(seed)).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e, false),
    }
}

#[derive(Deserialize)]
struct ViewQuery {
    player: Option<u32>,
    since: Option<u64>,
    token: Option<String>,
}

/// Seat of the requester: the token decides (a wrong player/token pair is refused)
fn seat_of(g: &ServerGame, q: &ViewQuery, headers: &axum::http::HeaderMap) -> Result<Option<usize>, Response> {
    let token = q.token.clone().or_else(|| {
        headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")).map(String::from)
    });
    let Some(token) = token.filter(|t| !t.is_empty()) else { return Ok(None) };
    let seats = g.seats();
    match seats.iter().position(|p| !p.3.is_empty() && p.3 == token) {
        Some(seat) if q.player.map(|id| seats[seat].0 == id).unwrap_or(true) => Ok(Some(seat)),
        _ => Err(error(StatusCode::FORBIDDEN, "Wrong token", false)),
    }
}

/// A game in memory, or loaded from disk (games are unloaded when idle)
async fn entry(s: &Shared, id: &str) -> Option<Arc<GameEntry>> {
    if let Some(e) = s.games.read().await.get(id).cloned() {
        e.last_access.store(now(), Ordering::Relaxed);
        return Some(e);
    }
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let bytes = tokio::fs::read(s.data_dir.join("games").join(format!("{}.json", id))).await.ok()?;
    let game = ServerGame::load(&bytes)?;
    let e = s.games.write().await.entry(id.to_string()).or_insert_with(|| GameEntry::new(game)).clone();
    // An AI may have a decision pending
    kick_ais(s, id, &e);
    Some(e)
}

/// Unloads saved games nobody looked at for a while: 1 minute once ended, 30 minutes otherwise
async fn unload_idle(s: &Shared) {
    let dirty = s.dirty.lock().await.clone();
    let mut games = s.games.write().await;
    let t = now();
    games.retain(|id, e| {
        let idle = t.saturating_sub(e.last_access.load(Ordering::Relaxed));
        let ended = e.game.try_lock().map(|g| g.ended()).unwrap_or(false);
        dirty.contains(id) || idle < if ended { 60 } else { 1800 }
    });
}

async fn view_game(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<ViewQuery>, headers: axum::http::HeaderMap) -> Response {
    let Some(e) = entry(&s, &id).await else { return error(StatusCode::NOT_FOUND, "Unknown game", false) };
    if let Some(since) = q.since {
        // Long polling: wait for a change (or 20 s)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let notified = e.changed.notified();
            if e.game.lock().await.seq() > since {
                break;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                break;
            }
        }
    }
    // Without a token: spectator view (no hidden cards, no decision)
    let seat = match seat_of(&*e.game.lock().await, &q, &headers) {
        Ok(seat) => seat,
        Err(r) => return r,
    };
    match with_game(&s, &e, false, move |g| g.view_json(seat)).await {
        Ok(Ok(view)) => json_bytes(view),
        Ok(Err(err)) | Err(err) => error(StatusCode::INTERNAL_SERVER_ERROR, err, false),
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
    let seat = match seat_of(&*e.game.lock().await, &q, &headers) {
        Ok(Some(seat)) => seat,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "A player token is required", false),
        Err(r) => return r,
    };
    let res = with_game(&s, &e, true, move |g| {
        g.answer(seat, &body)?;
        g.run_bots();
        Ok::<_, engines::Refused>(g.view_json(Some(seat)))
    })
    .await;
    match res {
        Ok(Ok(Ok(view))) => {
            e.changed.notify_waiters();
            kick_ais(&s, &id, &e);
            s.dirty.lock().await.insert(id);
            json_bytes(view)
        }
        Ok(Ok(Err(err))) | Err(err) => {
            // The answer was played: the view failed
            e.changed.notify_waiters();
            s.dirty.lock().await.insert(id);
            error(StatusCode::INTERNAL_SERVER_ERROR, err, false)
        }
        Ok(Err(refused)) => error(StatusCode::from_u16(refused.status).unwrap_or(StatusCode::BAD_REQUEST), refused.msg, refused.user),
    }
}

// ---------------------------------------------------------------------------
// AI seats: the server plays their decisions in the background, one at a time
// ---------------------------------------------------------------------------

/// Starts the task that plays the AI seats' decisions until none is pending (unless it runs)
fn kick_ais(s: &Shared, id: &str, e: &Arc<GameEntry>) {
    if !e.has_ai {
        return;
    }
    e.ai_kicks.fetch_add(1, Ordering::SeqCst);
    if e.ai_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let (s, id, e) = (s.clone(), id.to_string(), e.clone());
    tokio::spawn(async move {
        loop {
            let kicks = e.ai_kicks.load(Ordering::SeqCst);
            match ai_step(&s, &e).await {
                Ok(true) => {
                    e.changed.notify_waiters();
                    s.dirty.lock().await.insert(id.clone());
                    continue;
                }
                Ok(false) => {}
                Err(err) => eprintln!("game {}: AI: {}", id, err),
            }
            e.ai_running.store(false, Ordering::SeqCst);
            // A kick during the last step (an answer that gave an AI a decision): look again
            if e.ai_kicks.load(Ordering::SeqCst) == kicks || e.ai_running.swap(true, Ordering::SeqCst) {
                break;
            }
        }
    });
}

/// One decision of an AI seat (false: none pending). The AI thinks on a copy of the game, without
/// its lock (an imitation AI takes up to a second): views and the other player go on meanwhile.
/// Its answer is played if the decision is still current, else the next step decides again.
async fn ai_step(s: &Shared, e: &Arc<GameEntry>) -> Result<bool, String> {
    let pending = with_game(s, e, false, |g| g.ai_decision().map(|(pid, d)| (pid, d, g.game().clone(), g.ai_seats()))).await?;
    let Some((pid, d, game, seats)) = pending else { return Ok(false) };
    let (players, s2, d2) = (e.ai.clone(), s.clone(), d.clone());
    let decided = s
        .pool
        .run(move || {
            let mut guard = players.lock().unwrap_or_else(|p| p.into_inner());
            let p = &mut *guard;
            if !p.agents.contains_key(&pid) {
                for (seat, seat_pid, ai) in &seats {
                    let agent = s2.ais.agent(ai).unwrap_or_else(|err| {
                        eprintln!("AI {}: {} (the simple bot plays instead)", ai, err);
                        Box::new(SimpleBot)
                    });
                    p.decks[*seat] = s2.ais.describe(ai).map(|(_, deck)| deck).unwrap_or_default();
                    p.agents.insert(*seat_pid, agent);
                }
            }
            let rng = p.rng.get_or_insert_with(|| ChaCha8Rng::seed_from_u64(seed()));
            let obs = Observation::new(&game, pid as i64, &p.decks);
            match p.agents.get_mut(&pid) {
                Some(agent) => {
                    let agent = agent.as_mut();
                    // A panic of the AI: it is built again at its next decision
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| agent.decide(&obs, &d2, rng))).map_err(|_| ())
                }
                None => Err(()),
            }
            .map_err(|_| {
                p.agents.remove(&pid);
            })
        })
        .await;
    // The AI failed: the simple bot's answer
    let a = match decided {
        Ok(Ok(a)) => a,
        _ => {
            eprintln!("AI of player {}: no answer to a {} decision, the simple bot answers", pid, d["kind"]);
            altered_ai::legal::candidates(&d).into_iter().next().unwrap_or(json!({"type": "pass"}))
        }
    };
    with_game(s, e, true, move |g| match g.ai_decision() {
        Some((p, current)) if p == pid && current["id"] == d["id"] => g.ai_answer(pid, &d, &a),
        _ => Ok(()),
    })
    .await??;
    Ok(true)
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
    if let Some(d) = &req.deck {
        if !known_deck(d) {
            return error(StatusCode::BAD_REQUEST, format!("Unknown deck {}", d), true);
        }
    }
    let mut queue = s.queue.lock().await;
    queue.retain(|_, t| t.matched.is_some() || now() - t.created < 120);
    let waiting = queue.iter().find(|(_, t)| t.matched.is_none()).map(|(k, t)| (k.clone(), t.clone()));
    match waiting {
        Some((other_id, other)) => {
            let id = new_game_id(&s);
            let seats = [
                NewSeat { name: other.name.clone(), bot: false, deck: other.deck.clone(), ai: None },
                NewSeat { name: name.clone(), bot: false, deck: req.deck.clone(), ai: None },
            ];
            let created = match insert_game(&s, id.clone(), seats, seed()).await {
                Ok(v) => v,
                Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e, false),
            };
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
// Debug: full hidden state (ARENA_DEBUG)
// ---------------------------------------------------------------------------

async fn debug_state(State(s): State<Shared>, Path(id): Path<String>) -> Response {
    let Some(e) = entry(&s, &id).await else { return error(StatusCode::NOT_FOUND, "Unknown game", false) };
    match with_game(&s, &e, false, |g| g.debug_state()).await {
        Ok(v) => Json(v).into_response(),
        Err(err) => error(StatusCode::INTERNAL_SERVER_ERROR, err, false),
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn save_dirty(s: &Shared) {
    let ids: Vec<String> = s.dirty.lock().await.drain().collect();
    for id in ids {
        let Some(e) = entry(s, &id).await else { continue };
        let json = e.game.lock().await.to_json();
        let path = s.data_dir.join("games").join(format!("{}.json", id));
        let tmp = path.with_extension("tmp");
        if tokio::fs::write(&tmp, json).await.is_ok() {
            let _ = tokio::fs::rename(&tmp, &path).await;
        }
    }
}
