//! Private rooms: a game against a friend.
//!
//! The host creates the room with their deck and sends the guest's link. In the room each player
//! chooses a deck (and may change it), then confirms that they are ready. Once both are ready, the
//! game is created under the room's id, with the room's seat tokens: the links stay valid. A room
//! never tells a player the other one's deck. Rooms live in memory (like the matchmaking queue);
//! once the game exists, a room that is gone answers that the game started.

use crate::engines::NewSeat;
use crate::{clean_name, entry, error, insert_game, known_deck, new_game_id, new_token, now, reunion, seed, Shared};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

/// The two seats of a room, as in a game
const PLAYERS: [u32; 2] = [2001, 2002];

/// A room not touched for this long is dropped (seconds); a started one sooner (the game holds it)
const IDLE: u64 = 24 * 3600;
const IDLE_STARTED: u64 = 600;

#[derive(Clone, Default)]
pub struct RoomSeat {
    token: String,
    name: String,
    /// The player opened the room (the host from the start)
    joined: bool,
    /// Starter deck id
    deck: Option<String>,
    /// Altered Reunion deck: its decks-api id, and the deck read (checked by the engine)
    reunion_id: Option<String>,
    reunion_deck: Option<Value>,
    ready: bool,
}

impl RoomSeat {
    fn has_deck(&self) -> bool {
        self.deck.is_some() || self.reunion_deck.is_some()
    }
}

pub struct Room {
    seats: [RoomSeat; 2],
    /// Increases with every change (long polling)
    seq: u64,
    last: u64,
    started: bool,
    /// Why the game could not start (both players are no longer ready)
    error: Option<String>,
}

/// Rooms to keep: the idle ones are dropped
pub fn keep(r: &Room) -> bool {
    now().saturating_sub(r.last) < if r.started { IDLE_STARTED } else { IDLE }
}

/// What a seat sees of its room: its own deck, only whether the other player has one
fn view(id: &str, r: &Room, seat: usize) -> Value {
    let mine = &r.seats[seat];
    let deck = match (&mine.deck, &mine.reunion_id) {
        (Some(d), _) => json!({"deck": d}),
        (None, Some(rid)) if mine.reunion_deck.is_some() => json!({"reunionDeck": rid}),
        _ => Value::Null,
    };
    let seats: Vec<Value> =
        r.seats.iter().zip(PLAYERS).map(|(s, pid)| json!({"id": pid, "name": s.name, "joined": s.joined, "ready": s.ready, "hasDeck": s.has_deck()})).collect();
    let mut v = json!({"roomId": id, "seq": r.seq, "you": PLAYERS[seat], "started": r.started, "seats": seats, "deck": deck});
    if let Some(e) = &r.error {
        v["error"] = json!(e);
    }
    v
}

/// The seat of the requester's token (required)
fn seat_of(r: &Room, q: &RoomQuery, headers: &HeaderMap) -> Result<usize, Response> {
    let token =
        q.token.clone().or_else(|| headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")).map(String::from));
    let Some(token) = token.filter(|t| !t.is_empty()) else { return Err(error(StatusCode::UNAUTHORIZED, "A player token is required", false)) };
    match r.seats.iter().position(|s| s.token == token) {
        Some(seat) if q.player.map(|p| PLAYERS[seat] == p).unwrap_or(true) => Ok(seat),
        _ => Err(error(StatusCode::FORBIDDEN, "Wrong token", false)),
    }
}

/// A Reunion deck read from decks-api and checked by the engine
async fn checked_reunion(s: &Shared, id: &str) -> Result<Value, Response> {
    let d = reunion::fetch(&s.http, id).await.map_err(|e| error(StatusCode::BAD_REQUEST, e, true))?;
    let d2 = d.clone();
    s.pool.run(move || reunion::check(&d2)).await.and_then(|c| c).map_err(|e| error(StatusCode::BAD_REQUEST, format!("Deck refused: {}", e), true))?;
    Ok(d)
}

/// The deck a player asks for: (starter deck, Reunion deck id and deck). None: no deck in the request.
async fn requested_deck(s: &Shared, deck: Option<&str>, reunion_deck: Option<&str>) -> Result<Option<(Option<String>, Option<(String, Value)>)>, Response> {
    if let Some(id) = reunion_deck {
        let d = checked_reunion(s, id).await?;
        return Ok(Some((None, Some((id.to_string(), d)))));
    }
    if let Some(d) = deck {
        if !known_deck(d) {
            return Err(error(StatusCode::BAD_REQUEST, format!("Unknown deck {}", d), true));
        }
        return Ok(Some((Some(d.to_string()), None)));
    }
    Ok(None)
}

fn set_deck(seat: &mut RoomSeat, (deck, reunion): (Option<String>, Option<(String, Value)>)) {
    seat.deck = deck;
    (seat.reunion_id, seat.reunion_deck) = match reunion {
        Some((id, d)) => (Some(id), Some(d)),
        None => (None, None),
    };
}

#[derive(Deserialize)]
pub struct NewRoom {
    name: Option<String>,
    deck: Option<String>,
    #[serde(rename = "reunionDeck")]
    reunion_deck: Option<String>,
}

/// POST /api/v1/rooms: the host's seat (2001) with their deck, not ready yet; both tokens go to the host
pub async fn create_room(State(s): State<Shared>, Json(req): Json<NewRoom>) -> Response {
    let mut host = RoomSeat { token: new_token(), name: clean_name(req.name.as_deref(), 0), joined: true, ..Default::default() };
    match requested_deck(&s, req.deck.as_deref(), req.reunion_deck.as_deref()).await {
        Ok(Some(d)) => set_deck(&mut host, d),
        Ok(None) => {}
        Err(r) => return r,
    }
    let guest = RoomSeat { token: new_token(), name: clean_name(None, 1), ..Default::default() };
    let tokens = [host.token.clone(), guest.token.clone()];
    let id = new_game_id(&s);
    s.rooms.lock().await.insert(id.clone(), Room { seats: [host, guest], seq: 1, last: now(), started: false, error: None });
    Json(json!({"roomId": id, "players": [{"id": PLAYERS[0], "token": tokens[0]}, {"id": PLAYERS[1], "token": tokens[1]}]})).into_response()
}

#[derive(Deserialize)]
pub struct RoomQuery {
    player: Option<u32>,
    since: Option<u64>,
    token: Option<String>,
}

/// GET /api/v1/rooms/{id}: the room as this seat sees it (opening it joins it). With `since`, waits
/// (up to 20 s) for a change.
pub async fn view_room(State(s): State<Shared>, Path(id): Path<String>, Query(q): Query<RoomQuery>, headers: HeaderMap) -> Response {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let notified = s.rooms_changed.notified();
        {
            let mut rooms = s.rooms.lock().await;
            let Some(r) = rooms.get_mut(&id) else {
                drop(rooms);
                // The room is gone (restart, or dropped once started): the game, if it exists, holds the seats
                return match entry(&s, &id).await {
                    Some(_) => Json(json!({"roomId": id, "seq": 0, "started": true, "seats": [], "deck": null})).into_response(),
                    None => error(StatusCode::NOT_FOUND, "Unknown room", true),
                };
            };
            let seat = match seat_of(r, &q, &headers) {
                Ok(seat) => seat,
                Err(resp) => return resp,
            };
            r.last = now();
            if !r.seats[seat].joined {
                r.seats[seat].joined = true;
                r.seq += 1;
                s.rooms_changed.notify_waiters();
            }
            if q.since.map(|since| r.seq > since).unwrap_or(true) {
                return Json(view(&id, r, seat)).into_response();
            }
        }
        if tokio::time::timeout_at(deadline, notified).await.is_err() {
            let rooms = s.rooms.lock().await;
            return match rooms.get(&id) {
                Some(r) => match seat_of(r, &q, &headers) {
                    Ok(seat) => Json(view(&id, r, seat)).into_response(),
                    Err(resp) => resp,
                },
                None => error(StatusCode::NOT_FOUND, "Unknown room", true),
            };
        }
    }
}

#[derive(Deserialize)]
pub struct SeatUpdate {
    name: Option<String>,
    deck: Option<String>,
    #[serde(rename = "reunionDeck")]
    reunion_deck: Option<String>,
    ready: Option<bool>,
}

/// POST /api/v1/rooms/{id}/seat: the player's name, deck (a new deck: no longer ready) and readiness.
/// When both players are ready, the game starts.
pub async fn update_seat(
    State(s): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<RoomQuery>,
    headers: HeaderMap,
    Json(req): Json<SeatUpdate>,
) -> Response {
    // Check the seat before reading a deck from decks-api
    {
        let rooms = s.rooms.lock().await;
        let Some(r) = rooms.get(&id) else { return error(StatusCode::NOT_FOUND, "Unknown room", true) };
        if let Err(resp) = seat_of(r, &q, &headers) {
            return resp;
        }
    }
    let deck = match requested_deck(&s, req.deck.as_deref(), req.reunion_deck.as_deref()).await {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut rooms = s.rooms.lock().await;
    let Some(r) = rooms.get_mut(&id) else { return error(StatusCode::NOT_FOUND, "Unknown room", true) };
    let seat = match seat_of(r, &q, &headers) {
        Ok(seat) => seat,
        Err(resp) => return resp,
    };
    if r.started {
        return error(StatusCode::CONFLICT, "The game has started", true);
    }
    let me = &mut r.seats[seat];
    me.joined = true;
    if let Some(n) = req.name.as_deref() {
        me.name = clean_name(Some(n), seat);
    }
    if let Some(d) = deck {
        set_deck(me, d);
        me.ready = false;
    }
    if let Some(ready) = req.ready {
        if ready && !me.has_deck() {
            return error(StatusCode::BAD_REQUEST, "Choose a deck first", true);
        }
        me.ready = ready;
    }
    r.error = None;
    r.seq += 1;
    r.last = now();
    let start = r.seats.iter().all(|x| x.ready && x.has_deck());
    if start {
        r.started = true;
    }
    let seats = r.seats.clone();
    let v = view(&id, r, seat);
    drop(rooms);
    s.rooms_changed.notify_waiters();
    if !start {
        return Json(v).into_response();
    }

    // Both ready: the game, under the room's id and with its tokens
    let tokens = [seats[0].token.clone(), seats[1].token.clone()];
    let new_seats = seats.map(|x| NewSeat { name: x.name, bot: false, deck: x.deck, ai: None, reunion_deck: x.reunion_deck });
    let res = insert_game(&s, id.clone(), new_seats, seed(), Some(tokens)).await;
    let mut rooms = s.rooms.lock().await;
    let Some(r) = rooms.get_mut(&id) else { return error(StatusCode::NOT_FOUND, "Unknown room", true) };
    if let Err(e) = res {
        r.started = false;
        r.error = Some(format!("The game could not start: {}", e));
        for x in r.seats.iter_mut() {
            x.ready = false;
        }
    }
    r.seq += 1;
    let v = view(&id, r, seat);
    drop(rooms);
    s.rooms_changed.notify_waiters();
    Json(v).into_response()
}
