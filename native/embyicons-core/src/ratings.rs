use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

#[derive(Default)]
pub struct Ratings {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    entries: HashMap<String, Value>,
    running: HashSet<String>,
    failures: HashMap<String, i64>,
    path: PathBuf,
    dirty: bool,
    enabled: bool,
}
impl Ratings {
    pub fn cached(&self, kind: &str, tmdb: &str) -> Value {
        let media = if kind == "Series" { "show" } else { "movie" };
        self.state
            .lock()
            .unwrap()
            .entries
            .get(&format!("mdblist_{media}_{tmdb}"))
            .map(|entry| entry["Data"].clone())
            .unwrap_or(Value::Null)
    }
    pub fn load(&self, path: PathBuf, enabled: bool) {
        let entries = if enabled {
            fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| v["Entries"].as_object().cloned())
                .unwrap_or_default()
        } else {
            Default::default()
        };
        let mut state = self.state.lock().unwrap();
        state.entries = entries.into_iter().collect();
        state.path = path;
        state.enabled = enabled;
    }
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap();
        if enabled && !state.enabled {
            if let Some(entries) = fs::read(&state.path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| v["Entries"].as_object().cloned())
            {
                for (k, v) in entries {
                    state.entries.entry(k).or_insert(v);
                }
            }
        }
        state.enabled = enabled;
    }
    pub fn resolve(&self, kind: &str, tmdb: &str, api_key: &str, enabled: bool) -> Value {
        if !enabled
            || api_key.is_empty()
            || tmdb.is_empty()
            || !tmdb.bytes().all(|b| b.is_ascii_digit())
        {
            return Value::Null;
        }
        let media = if kind == "Series" { "show" } else { "movie" };
        let key = format!("mdblist_{media}_{tmdb}");
        let now = Utc::now().timestamp();
        let stale = {
            let mut state = self.state.lock().unwrap();
            let cached = state.entries.get(&key).cloned().unwrap_or(Value::Null);
            let at = cached["CachedAt"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.timestamp())
                .unwrap_or(0);
            if now - at < 7 * 86400
                || state.failures.get(&key).is_some_and(|t| *t > now)
                || !state.running.insert(key.clone())
            {
                return cached["Data"].clone();
            }
            cached["Data"].clone()
        };
        // Never hold the core state or a drawing slot during this bounded request.
        let result = (|| -> Result<Value, ()> {
            static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
            let agent = AGENT.get_or_init(|| {
                ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(15)))
                    .build()
                    .new_agent()
            });
            let url = format!("https://api.mdblist.com/tmdb/{media}/{tmdb}");
            let mut response = match agent.get(&url).query("apikey", api_key).call() {
                Ok(r) => r,
                Err(ureq::Error::StatusCode(404)) => return Ok(json!({})),
                Err(_) => return Err(()),
            };
            let body: Value = response.body_mut().read_json().map_err(|_| ())?;
            let mut data = json!({});
            if let Some(rows) = body["ratings"].as_array() {
                for row in rows {
                    let source = row["source"].as_str().unwrap_or("").to_lowercase();
                    if source.contains("popcorn") || source.contains("audience") {
                        if row["value"].is_number() {
                            data["PopcornScore"] = row["value"].clone();
                        }
                        if row["votes"].is_number() {
                            data["PopcornVotes"] = row["votes"].clone();
                        }
                    } else if source.contains("myanimelist") || source.contains("mal") {
                        if row["value"].is_number() {
                            data["MyAnimeListScore"] = row["value"].clone();
                        }
                    }
                }
            }
            Ok(data)
        })();
        let mut state = self.state.lock().unwrap();
        state.running.remove(&key);
        match result {
            Ok(data) => {
                state.entries.insert(
                    key.clone(),
                    json!({"Data":data,"CachedAt":Utc::now().to_rfc3339()}),
                );
                state.failures.remove(&key);
                state.dirty = true;
                data
            }
            Err(()) => {
                state.failures.insert(key, now + 3600);
                stale
            }
        }
    }
    pub fn save(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "ratings lock poisoned")?;
        if !state.dirty || !state.enabled || state.path.as_os_str().is_empty() {
            return Ok(());
        }
        if state.entries.len() > 50000 {
            let mut keys: Vec<_> = state
                .entries
                .iter()
                .map(|(k, v)| (k.clone(), v["CachedAt"].as_str().unwrap_or("").to_owned()))
                .collect();
            keys.sort_by(|a, b| a.1.cmp(&b.1));
            for (key, _) in keys.into_iter().take(state.entries.len() - 50000) {
                state.entries.remove(&key);
            }
        }
        let bytes = serde_json::to_vec(&json!({"Version":1,"Entries":state.entries}))
            .map_err(|e| e.to_string())?;
        crate::engine::atomic_save(&state.path, &bytes)?;
        state.dirty = false;
        Ok(())
    }
}
