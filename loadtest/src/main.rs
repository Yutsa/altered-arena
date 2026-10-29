//! Load test: keeps N games running at the same time against an Altered Arena server, each
//! human seat played by a simulated client that behaves like the web client (long polling,
//! answers after a think time), and reports latency, throughput, and the server's RSS / CPU.
//!
//! Usage: loadtest --games 1000 --pvp 0.5 --think-ms 1500 --duration 60 --url http://localhost:8090 --pid <server pid>
//!   --pvp: share of games between two simulated humans (the rest are against the server bot)

use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

struct Stats {
    latencies_us: Mutex<Vec<u64>>,
    answers: AtomicU64,
    polls: AtomicU64,
    errors: AtomicU64,
    games_done: AtomicU64,
    decisions_refused: AtomicU64,
    /// Refused answers by decision kind and error message
    refused_reasons: Mutex<std::collections::BTreeMap<String, u64>>,
}

fn arg<T: std::str::FromStr>(name: &str, default: T) -> T {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Cheap deterministic randomness per client
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

fn pick_answer(d: &Value, rng: &mut Lcg) -> Value {
    let id = d["id"].clone();
    let arr = |k: &str| d[k].as_array().cloned().unwrap_or_default();
    let a = match d["kind"].as_str().unwrap_or("") {
        "turn" => {
            let play = arr("play");
            if !play.is_empty() && rng.below(10) < 8 {
                let p = &play[rng.below(play.len())];
                let dests = p["destinations"].as_array().cloned().unwrap_or_default();
                json!({"type": "play", "cardId": p["cardId"], "destination": dests[rng.below(dests.len())]})
            } else if d["canEndTurn"].as_bool().unwrap_or(false) {
                json!({"type": "endTurn"})
            } else {
                json!({"type": "pass"})
            }
        }
        "select" => {
            let mut ids = arr("cardIds");
            let max = d["max"].as_u64().unwrap_or(1) as usize;
            for i in (1..ids.len()).rev() {
                ids.swap(i, rng.below(i + 1));
            }
            ids.truncate(max);
            json!({"type": "select", "cardIds": ids})
        }
        "choice" => {
            let en: Vec<Value> = arr("choices").into_iter().filter(|c| c["enabled"].as_bool().unwrap_or(false)).collect();
            if en.is_empty() { json!({"type": "pass"}) } else { json!({"type": "choice", "id": en[rng.below(en.len())]["id"]}) }
        }
        "expedition" => {
            let e = arr("expeditions");
            let n = d["min"].as_u64().unwrap_or(1).max(1) as usize;
            json!({"type": "expedition", "keys": e.iter().take(n).map(|x| x["key"].clone()).collect::<Vec<_>>()})
        }
        "player" if !arr("players").is_empty() => json!({"type": "player", "player": arr("players")[0]}),
        "number" => json!({"type": "number", "value": arr("values").last().cloned().unwrap_or(json!(0))}),
        "location" if !arr("locations").is_empty() => json!({"type": "location", "key": arr("locations")[0]["key"]}),
        "exchange" if !arr("hand").is_empty() && !arr("reserve").is_empty() => json!({"type": "exchange", "hand": arr("hand")[0], "reserve": arr("reserve")[0]}),
        "cleanup" => {
            let r = d["reserve"]["cardIds"].as_array().cloned().unwrap_or_default();
            let l = d["landmarks"]["cardIds"].as_array().cloned().unwrap_or_default();
            let nr = d["reserve"]["n"].as_u64().unwrap_or(0) as usize;
            let nl = d["landmarks"]["n"].as_u64().unwrap_or(0) as usize;
            json!({"type": "cleanup", "reserve": r[..nr.min(r.len())], "landmarks": l[..nl.min(l.len())]})
        }
        // Decisions of the translated engine (rules)
        "confirm" => json!({"type": "confirm"}),
        "deck" if !arr("decks").is_empty() => json!({"type": "deck", "deck": arr("decks")[rng.below(arr("decks").len())]["id"]}),
        "buttons" if !arr("buttons").is_empty() => json!({"type": "button", "key": arr("buttons")[rng.below(arr("buttons").len())]["key"]}),
        _ => json!({"type": "pass"}),
    };
    let mut a = a;
    a["decisionId"] = id;
    a
}

/// One simulated player: plays its seat until the game ends or time runs out
async fn play_seat(client: reqwest::Client, url: String, game: String, player: u64, token: String, think: u64, until: Instant, stats: Arc<Stats>, seed: u64) {
    let mut rng = Lcg(seed);
    let base = format!("{}/api/v1/games/{}", url, game);
    let auth = format!("player={}&token={}", player, token);
    let mut view: Value = match client.get(format!("{}?{}", base, auth)).send().await {
        Ok(r) => r.json().await.unwrap_or(Value::Null),
        Err(_) => {
            stats.errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };
    while Instant::now() < until {
        if view["phase"] == "ended" {
            stats.games_done.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if view["decision"].is_null() {
            // Long poll for a change
            let seq = view["seq"].as_u64().unwrap_or(0);
            let t = Instant::now();
            match client.get(format!("{}?{}&since={}", base, auth, seq)).send().await {
                Ok(r) => {
                    stats.polls.fetch_add(1, Ordering::Relaxed);
                    let _ = t;
                    view = r.json().await.unwrap_or(view);
                }
                Err(_) => {
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
            continue;
        }
        // Think, then answer
        let jitter = if think > 0 { rng.below(think as usize) as u64 } else { 0 };
        tokio::time::sleep(Duration::from_millis(think / 2 + jitter)).await;
        let answer = pick_answer(&view["decision"], &mut rng);
        let t = Instant::now();
        match client.post(format!("{}/answer?{}", base, auth)).json(&answer).send().await {
            Ok(r) => {
                let status = r.status();
                let body: Value = r.json().await.unwrap_or(Value::Null);
                stats.latencies_us.lock().await.push(t.elapsed().as_micros() as u64);
                stats.answers.fetch_add(1, Ordering::Relaxed);
                if status.is_success() {
                    view = body;
                } else {
                    stats.decisions_refused.fetch_add(1, Ordering::Relaxed);
                    let reason = format!("{} {}: {}", view["decision"]["kind"].as_str().unwrap_or("?"), answer["type"].as_str().unwrap_or("?"), body["error"].as_str().unwrap_or("?"));
                    *stats.refused_reasons.lock().await.entry(reason).or_insert(0) += 1;
                    // Retry with a fresh view (and a pass if possible)
                    if let Ok(r) = client.get(format!("{}?{}", base, auth)).send().await {
                        view = r.json().await.unwrap_or(view);
                    }
                    if view["decision"]["canPass"].as_bool().unwrap_or(false) {
                        let pass = json!({"type": "pass", "decisionId": view["decision"]["id"]});
                        if let Ok(r) = client.post(format!("{}/answer?{}", base, auth)).json(&pass).send().await {
                            if r.status().is_success() {
                                view = r.json().await.unwrap_or(view);
                            }
                        }
                    }
                }
            }
            Err(_) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

/// Server CPU time used so far, in seconds (ps TIME: [[dd-]hh:]mm:ss.cc)
fn cpu_seconds(pid: u32) -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "time=", "-p", &pid.to_string()]).output();
    let Ok(out) = out else { return 0.0 };
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.split(':').fold(0.0, |acc, part| acc * 60.0 + part.parse::<f64>().unwrap_or(0.0))
}

/// A file of the server's container (cgroup v2 / network statistics)
fn docker_cat(name: &str, file: &str) -> String {
    std::process::Command::new("docker").args(["exec", name, "cat", file]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

fn stat_field(text: &str, key: &str) -> u64 {
    text.lines().find_map(|l| l.strip_prefix(key).and_then(|v| v.trim().parse().ok())).unwrap_or(0)
}

/// CPU seconds used by the container
fn docker_cpu_seconds(name: &str) -> f64 {
    stat_field(&docker_cat(name, "/sys/fs/cgroup/cpu.stat"), "usage_usec ") as f64 / 1e6
}

/// Memory of the container in KB: (anonymous memory, as RSS; total charged to the cgroup)
fn docker_mem_kb(name: &str) -> (u64, u64) {
    let anon = stat_field(&docker_cat(name, "/sys/fs/cgroup/memory.stat"), "anon ") / 1024;
    let current = docker_cat(name, "/sys/fs/cgroup/memory.current").trim().parse::<u64>().unwrap_or(0) / 1024;
    (anon, current)
}

fn docker_tx_bytes(name: &str) -> u64 {
    docker_cat(name, "/sys/class/net/eth0/statistics/tx_bytes").trim().parse().unwrap_or(0)
}

fn sample_process(pid: u32) -> Option<(u64, f64)> {
    let out = std::process::Command::new("ps").args(["-o", "rss=,%cpu=", "-p", &pid.to_string()]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut it = s.split_whitespace();
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

#[tokio::main]
async fn main() {
    let url: String = arg("--url", "http://localhost:8090".to_string());
    let games: usize = arg("--games", 100);
    let pvp: f64 = arg("--pvp", 0.5);
    let think: u64 = arg("--think-ms", 1500);
    let duration: u64 = arg("--duration", 60);
    let pid: u32 = arg("--pid", 0);
    // The server runs in this docker container (CPU, memory and bytes sent read from its cgroup)
    let docker: String = arg("--docker", String::new());
    let ramp: u64 = arg("--ramp", 10);

    let client = reqwest::Client::builder().pool_max_idle_per_host(4096).timeout(Duration::from_secs(30)).build().unwrap();
    let stats = Arc::new(Stats {
        latencies_us: Mutex::new(Vec::new()),
        answers: AtomicU64::new(0),
        polls: AtomicU64::new(0),
        errors: AtomicU64::new(0),
        games_done: AtomicU64::new(0),
        decisions_refused: AtomicU64::new(0),
        refused_reasons: Mutex::new(Default::default()),
    });
    let decks: Vec<Value> = client.get(format!("{}/api/v1/decks", url)).send().await.unwrap().json().await.unwrap();
    let start = Instant::now();
    let until = start + Duration::from_secs(duration);
    let base_rss = if pid > 0 { sample_process(pid).map(|s| s.0).unwrap_or(0) } else if !docker.is_empty() { docker_mem_kb(&docker).0 } else { 0 };
    let cpu_start = if pid > 0 { cpu_seconds(pid) } else if !docker.is_empty() { docker_cpu_seconds(&docker) } else { 0.0 };
    let tx_start = if docker.is_empty() { 0 } else { docker_tx_bytes(&docker) };
    let max_mem_current = Arc::new(AtomicU64::new(0));

    // Sampler
    let samples = Arc::new(Mutex::new(Vec::<(f64, u64, f64)>::new()));
    if pid > 0 {
        let samples = samples.clone();
        tokio::spawn(async move {
            while Instant::now() < until {
                if let Some((rss, cpu)) = sample_process(pid) {
                    samples.lock().await.push((start.elapsed().as_secs_f64(), rss, cpu));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    } else if !docker.is_empty() {
        let (samples, docker, max_cur) = (samples.clone(), docker.clone(), max_mem_current.clone());
        tokio::spawn(async move {
            let mut last = (start.elapsed().as_secs_f64(), docker_cpu_seconds(&docker));
            while Instant::now() < until {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let d = docker.clone();
                let Ok((cpu, (anon, cur))) = tokio::task::spawn_blocking(move || (docker_cpu_seconds(&d), docker_mem_kb(&d))).await else { continue };
                let t = start.elapsed().as_secs_f64();
                let pct = 100.0 * (cpu - last.1) / (t - last.0).max(0.001);
                last = (t, cpu);
                max_cur.fetch_max(cur, Ordering::Relaxed);
                samples.lock().await.push((t, anon, pct));
            }
        });
    }

    // Games, created over the ramp-up period; a finished game is replaced by a new one
    let create_lat = Arc::new(Mutex::new(Vec::<u64>::new()));
    let mut handles = vec![];
    for i in 0..games {
        let (client, url, stats, decks, create_lat) = (client.clone(), url.clone(), stats.clone(), decks.clone(), create_lat.clone());
        let delay = Duration::from_millis(ramp * 1000 * i as u64 / games.max(1) as u64);
        let is_pvp = (i as f64) < pvp * games as f64;
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let mut round = 0u64;
            while Instant::now() < until {
                let d0 = decks[(i + round as usize) % decks.len()]["id"].clone();
                let d1 = decks[(i * 7 + 3 + round as usize) % decks.len()]["id"].clone();
                let body = json!({"players": [{"name": "Load A", "deck": d0}, {"name": "Load B", "deck": d1, "bot": !is_pvp}]});
                let t = Instant::now();
                let Ok(r) = client.post(format!("{}/api/v1/games", url)).json(&body).send().await else {
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                create_lat.lock().await.push(t.elapsed().as_micros() as u64);
                let g: Value = r.json().await.unwrap_or(Value::Null);
                let game = g["gameId"].as_str().unwrap_or("").to_string();
                let mut seats = vec![];
                for p in g["players"].as_array().cloned().unwrap_or_default() {
                    if p["bot"].as_bool().unwrap_or(false) {
                        continue;
                    }
                    let (client, url, game, stats) = (client.clone(), url.clone(), game.clone(), stats.clone());
                    let token = p["token"].as_str().unwrap_or("").to_string();
                    let player = p["id"].as_u64().unwrap_or(0);
                    let seed = (i as u64) << 20 | round << 4 | player % 16;
                    seats.push(tokio::spawn(play_seat(client, url, game, player, token, think, until, stats, seed)));
                }
                for s in seats {
                    let _ = s.await;
                }
                round += 1;
            }
        }));
    }
    // Progress
    let s2 = stats.clone();
    let progress = tokio::spawn(async move {
        while Instant::now() < until {
            tokio::time::sleep(Duration::from_secs(10)).await;
            eprintln!(
                "[{:>4.0}s] answers {} polls {} games finished {} errors {}",
                start.elapsed().as_secs_f64(),
                s2.answers.load(Ordering::Relaxed),
                s2.polls.load(Ordering::Relaxed),
                s2.games_done.load(Ordering::Relaxed),
                s2.errors.load(Ordering::Relaxed)
            );
        }
    });
    for h in handles {
        let _ = h.await;
    }
    progress.abort();

    let elapsed = start.elapsed().as_secs_f64();
    let cpu_used = if pid > 0 { cpu_seconds(pid) - cpu_start } else if !docker.is_empty() { docker_cpu_seconds(&docker) - cpu_start } else { 0.0 };
    let tx = if docker.is_empty() { 0 } else { docker_tx_bytes(&docker) - tx_start };
    let mut lat = stats.latencies_us.lock().await.clone();
    lat.sort_unstable();
    let pct = |p: f64| if lat.is_empty() { 0.0 } else { lat[((lat.len() as f64 - 1.0) * p) as usize] as f64 / 1000.0 };
    let mut cl = create_lat.lock().await.clone();
    cl.sort_unstable();
    let cpct = |p: f64| if cl.is_empty() { 0.0 } else { cl[((cl.len() as f64 - 1.0) * p) as usize] as f64 / 1000.0 };
    let samples = samples.lock().await.clone();
    // Steady state: after the ramp-up
    let steady: Vec<&(f64, u64, f64)> = samples.iter().filter(|s| s.0 > ramp as f64 + 2.0).collect();
    let avg_cpu = if steady.is_empty() { 0.0 } else { steady.iter().map(|s| s.2).sum::<f64>() / steady.len() as f64 };
    let max_rss = samples.iter().map(|s| s.1).max().unwrap_or(0);
    let result = json!({
        "games": games, "pvp": pvp, "thinkMs": think, "durationS": duration,
        "answers": stats.answers.load(Ordering::Relaxed),
        "answersPerSec": stats.answers.load(Ordering::Relaxed) as f64 / elapsed,
        "polls": stats.polls.load(Ordering::Relaxed),
        "gamesFinished": stats.games_done.load(Ordering::Relaxed),
        "errors": stats.errors.load(Ordering::Relaxed),
        "refused": stats.decisions_refused.load(Ordering::Relaxed),
        "answerLatencyMs": {"p50": pct(0.5), "p95": pct(0.95), "p99": pct(0.99), "max": pct(1.0)},
        "createLatencyMs": {"p50": cpct(0.5), "p95": cpct(0.95), "p99": cpct(0.99)},
        "serverRssMB": {"idle": base_rss as f64 / 1024.0, "max": max_rss as f64 / 1024.0},
        "serverCpuPercentAvg": avg_cpu,
        "serverCpuSeconds": cpu_used,
        "serverCpuMsPerAnswer": if stats.answers.load(Ordering::Relaxed) > 0 { cpu_used * 1000.0 / stats.answers.load(Ordering::Relaxed) as f64 } else { 0.0 },
        "serverRssKBPerGame": (max_rss.saturating_sub(base_rss)) as f64 / games.max(1) as f64,
        "refusedReasons": stats.refused_reasons.lock().await.clone(),
        "serverMemCurrentMBMax": max_mem_current.load(Ordering::Relaxed) as f64 / 1024.0,
        "serverTxBytes": tx,
        "serverTxBytesPerRequest": tx as f64 / (stats.answers.load(Ordering::Relaxed) + stats.polls.load(Ordering::Relaxed)).max(1) as f64,
        "serverTxMbitPerSec": tx as f64 * 8.0 / 1e6 / elapsed,
    });
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}
