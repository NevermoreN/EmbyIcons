mod engine;
mod legacy;
mod management;
mod ratings;
mod render;

use engine::Engine;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, slice, sync::Mutex};

const ABI_VERSION: u32 = 2;
const KEY_CAPACITY: usize = 512;
#[repr(C)]
pub struct Buffer {
    pub data: *mut u8,
    pub length: usize,
}
#[repr(C)]
pub struct QueryResult {
    pub generation: u64,
    pub ready: u32,
    pub key_length: u32,
    pub key: [u8; KEY_CAPACITY],
}
pub struct Context {
    engine: Mutex<Engine>,
    ratings: ratings::Ratings,
    save: Mutex<()>,
}

fn execute(context: &Context, command: Value) -> Result<Value, String> {
    let op = command["op"].as_str().ok_or("missing command op")?;
    let id = command["id"].as_u64().unwrap_or(0);
    let generation = command["generation"].as_u64().unwrap_or(0);
    if op == "hash" {
        return Ok(json!(engine::hash(render::text(&command, "text", ""))));
    }
    if op == "observe" {
        let item = command["item"].clone();
        let sequence = command["sequence"]
            .as_u64()
            .ok_or("missing event sequence")?;
        let (profile, config, previous, legacy) = {
            let core = context.engine.lock().map_err(|_| "core lock poisoned")?;
            (
                core.profile(&item),
                core.config.clone(),
                core.records.get(&id).cloned(),
                core.legacy.get(&id).cloned(),
            )
        };
        let Some(profile) = profile else {
            return context.engine.lock().unwrap().observed(
                id,
                sequence,
                item,
                None,
                Value::Null,
                String::new(),
                String::new(),
            );
        };
        if !engine::supported(&item, &profile["Settings"]) {
            return context.engine.lock().unwrap().observed(
                id,
                sequence,
                item,
                None,
                Value::Null,
                String::new(),
                String::new(),
            );
        }
        let settings = &profile["Settings"];
        let (stamp, digest) = match engine::source(&item, previous.as_ref()) {
            Ok(source) => source,
            Err(error) => {
                let mut core = context.engine.lock().unwrap();
                core.failed(id, 0, &error);
                if let Some(record) = core.records.get_mut(&id) {
                    record.waiting_input = true;
                }
                return Ok(json!({"waiting_input":true}));
            }
        };
        let external = render::alignment(settings, "PopcornScoreIconAlignment") > 0
            || render::alignment(settings, "MyAnimeListScoreIconAlignment") > 0;
        let provider = item["provider_ids"]
            .as_object()
            .and_then(|ids| ids.iter().find(|(key, _)| key.eq_ignore_ascii_case("Tmdb")))
            .and_then(|(_, v)| v.as_str())
            .unwrap_or("");
        // Adopt the previous renderer's persisted rating snapshot when probing
        // an old artifact. Fetching fresh scores first could mark that old
        // artifact as containing new scores it was never rendered with.
        let adopting = legacy.as_ref().is_some_and(|row| {
            previous
                .as_ref()
                .is_none_or(|record| record.generation == 0)
                && legacy::matches_metadata(row, &item, settings)
        });
        let scores = if external && adopting {
            context
                .ratings
                .cached(render::text(&item, "kind", ""), provider)
        } else {
            context.ratings.resolve(
                render::text(&item, "kind", ""),
                provider,
                render::text(&config, "MDBListApiKey", ""),
                external,
            )
        };
        let mut data = json!({});
        for (group, key, value) in [
            ("CommunityScore", "community", metadata_rating(&item, false)),
            (
                "RottenTomatoesScore",
                "critic",
                metadata_rating(&item, true),
            ),
            ("PopcornScore", "popcorn", scores["PopcornScore"].clone()),
            (
                "MyAnimeListScore",
                "mal",
                scores["MyAnimeListScore"].clone(),
            ),
        ] {
            if render::alignment(settings, &format!("{group}IconAlignment")) > 0 {
                data[key] = value;
            }
        }
        if render::alignment(settings, "PopcornScoreIconAlignment") > 0 {
            data["popcorn_votes"] = scores["PopcornVotes"].clone();
        }
        if render::alignment(settings, "ParentalRatingIconAlignment") > 0 {
            data["parental"] = json!(render::text(&item, "official_rating", "")
                .to_lowercase()
                .replace('/', "-"));
        }
        if render::alignment(settings, "FavoriteCountIconAlignment") > 0 {
            data["favorites"] = json!(item["favorite_users"]
                .as_array()
                .map(|a| a.iter().filter(|v| v.as_bool() == Some(true)).count())
                .unwrap_or(0));
        }
        return context.engine.lock().unwrap().observed(
            id,
            sequence,
            item,
            Some(profile),
            data,
            stamp,
            digest,
        );
    }
    if op == "render" {
        let (record, cache, smoothing) = {
            let core = context.engine.lock().unwrap();
            let record = core.records.get(&id).ok_or("render item was removed")?;
            if record.running != Some(generation) || record.generation != generation {
                return Err("render version was superseded".into());
            }
            (
                record.clone(),
                core.cache.clone(),
                render::flag(&core.config, "EnableImageSmoothing", false),
            )
        };
        if record.legacy_path {
            let mut core = context.engine.lock().unwrap();
            if let Some(current) = core.records.get_mut(&id) {
                if current.running == Some(generation) {
                    current.running = None;
                    current.generation += 1;
                    current.legacy_path = false;
                    current.target_key = format!("ei_r1_i{id}_{}", current.fingerprint);
                }
            }
            core.enqueue(id, true);
            return Ok(json!({"legacy_miss":true}));
        }
        context.engine.lock().unwrap().render_calls += 1;
        let start = engine::now_millis();
        let result = render::draw(
            render::text(&command, "input", ""),
            render::text(&record.item, "original", ""),
            render::text(&command, "output", ""),
            cache
                .join("enhanced-images")
                .to_str()
                .ok_or("cache path is not UTF-8")?,
            &record.settings,
            &record.data,
            smoothing,
        )?;
        let mut core = context.engine.lock().unwrap();
        core.draw_ms += engine::now_millis() - start;
        if let Some(record) = core.records.get_mut(&id) {
            if record.running == Some(generation) {
                record.rendered = true;
            }
        }
        return Ok(result);
    }
    if op == "maintenance" {
        let snapshot = context.engine.lock().unwrap().maintenance_snapshot();
        for (id, generation, ready, path) in snapshot {
            let missing = ready && !path.is_empty() && !std::path::Path::new(&path).is_file();
            context
                .engine
                .lock()
                .unwrap()
                .maintenance_result(id, generation, missing);
        }
        return Ok(Value::Null);
    }
    if op == "save" {
        let _gate = context.save.lock().map_err(|_| "save lock poisoned")?;
        let snapshot = context.engine.lock().unwrap().save_snapshot();
        let mut failures = Vec::new();
        for (bucket, path, data) in snapshot {
            if engine::atomic_save(&path, &data).is_err() {
                context.engine.lock().unwrap().dirty.insert(bucket);
                failures.push(bucket);
            }
        }
        context.ratings.save()?;
        if !failures.is_empty() {
            return Err(format!(
                "could not save {} native partitions",
                failures.len()
            ));
        }
        return Ok(Value::Null);
    }
    if op == "commit" {
        let previous = context.engine.lock().unwrap().records.get(&id).cloned();
        if let Some(record) = previous.filter(|r| !r.source_stamp.is_empty()) {
            let unchanged = engine::source(&record.item, Some(&record))
                .is_ok_and(|(stamp, _)| stamp == record.source_stamp);
            if !unchanged {
                let mut core = context.engine.lock().unwrap();
                if let Some(current) = core.records.get_mut(&id) {
                    if current.running == Some(generation) {
                        current.running = None;
                        current.generation += 1;
                    }
                }
                core.enqueue(id, true);
                return Ok(json!({"published":false}));
            }
        }
    }
    if op == "init" {
        let result = context.engine.lock().unwrap().initialize(&command)?;
        context.ratings.load(
            std::path::PathBuf::from(render::text(&command, "ratings_path", "")),
            render::flag(&command["config"], "EnableMdbListDiskCache", true),
        );
        return Ok(result);
    }
    if op == "preview" {
        let (cache, smoothing) = {
            let core = context.engine.lock().unwrap();
            (
                core.cache.clone(),
                render::flag(&core.config, "EnableImageSmoothing", false),
            )
        };
        let folder = cache.join("embyicons-preview");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let original = folder.join("source.png");
        if !original.exists() {
            std::fs::write(&original, include_bytes!("../../../Images/preview.png"))
                .map_err(|e| e.to_string())?;
        }
        let output = folder.join(format!("{}.png", uuid::Uuid::new_v4()));
        let data = json!({"community":6.9,"critic":88,"popcorn":92,"popcorn_votes":750,"mal":8.5,"favorites":42,"parental":"pg-13"});
        render::draw(
            original.to_str().unwrap(),
            original.to_str().unwrap(),
            output.to_str().unwrap(),
            folder.to_str().unwrap(),
            &command["settings"],
            &data,
            smoothing,
        )?;
        return Ok(json!({"path":output}));
    }
    let mut core = context.engine.lock().map_err(|_| "core lock poisoned")?;
    match op {
        "configure" => {
            context.ratings.set_enabled(render::flag(
                &command["config"],
                "EnableMdbListDiskCache",
                true,
            ));
            core.configure(&command["config"], &command["libraries"])
        }
        "validate_config" => {
            engine::validate_config(&command["config"])?;
            Ok(json!({"ok":true}))
        }
        "notify" => Ok(core.notify(&command)),
        "capacity" => Ok(json!({"backpressure":core.scan_backpressure()})),
        "limits" => Ok(json!({"workers":4,"render_slots":1})),
        "pilot" => {
            core.pilot = command["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_u64())
                .collect();
            core.reconcile = true;
            Ok(Value::Null)
        }
        "event" => {
            if render::flag(&command, "removed", false) {
                core.records.remove(&id);
                core.dirty.insert(engine::partition(id));
            } else if ["Movie", "Series", "BoxSet"].contains(&render::text(&command, "kind", "")) {
                if render::flag(&command, "favorite_change", false) {
                    core.legacy.remove(&id);
                    if let Some(record) = core.records.get_mut(&id).filter(|r| r.legacy_path) {
                        record.legacy_path = false;
                        record.ready = false;
                        record.generation = record.generation.saturating_add(1);
                        record.target_key = format!("ei_r1_i{id}_{}", record.fingerprint);
                        core.dirty.insert(engine::partition(id));
                    }
                }
                core.enqueue(id, true);
                core.prioritize(id);
            }
            if let Some(parent) = command["series_id"].as_u64().filter(|id| *id > 0) {
                core.enqueue(parent, true);
            }
            Ok(Value::Null)
        }
        "next" => Ok(core.next()),
        "begin" => core.begin(id),
        "commit" => core.commit(id, generation, render::text(&command, "path", "")),
        "missing" => core.missing(id, generation),
        "fail" => Ok(core.failed(
            id,
            generation,
            render::text(&command, "error", "background image job failed"),
        )),
        "cancel" => {
            core.failed(id, generation, "cancelled");
            Ok(Value::Null)
        }
        "remove" => {
            core.records.remove(&id);
            core.dirty.insert(engine::partition(id));
            Ok(Value::Null)
        }
        "stats" => Ok(core.stats()),
        "inspect" => Ok(match core.records.get(&id) {
            Some(r) => {
                json!({"Id":id,"Ready":r.ready,"Generation":r.published_generation,"Key":r.published_key,"Path":r.path,"Original":r.item["original"],"OriginalHash":r.source_hash,"LegacyPath":r.legacy_path,"WaitingInput":r.waiting_input,"Error":r.error})
            }
            None => json!({"Id":id,"Ready":false,"Missing":true}),
        }),
        "manage" => management::run(&mut core, &command),
        "reconcile" => {
            let needed = core.reconcile;
            if render::flag(&command, "ack", false) {
                core.reconcile = false;
            }
            Ok(json!({"needed":needed}))
        }
        "scopes" => {
            let ids: Vec<_> = core.config["LibraryProfileMappings"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|m| m["LibraryId"].clone())
                .collect();
            Ok(
                json!({"libraries":ids,"collections":render::flag(&core.config,"EnableCollectionProfileLookup",true),"pilot":core.pilot}),
            )
        }
        "batch_status" => Ok(core.batch_status(render::text(&command, "batch", ""))),
        "batch_finish" => {
            let mut result = core.batch_status(render::text(&command, "batch", ""));
            result["deferred"] = json!(
                result["deferred"].as_u64().unwrap_or(0) + result["pending"].as_u64().unwrap_or(0)
            );
            result["pending"] = json!(0);
            core.last_manual = result.clone();
            Ok(result)
        }
        "batch_end" => Ok(core.batch_end(
            render::text(&command, "batch", ""),
            render::flag(&command, "cancel", false),
        )),
        "stage" => {
            // Retained solely for the small host contract probe, not production keys.
            let key = render::text(&command, "legacy_key", "");
            if id == 0 || key.is_empty() || key.len() > KEY_CAPACITY - 64 {
                return Err("invalid item identity or key length".into());
            }
            let fingerprint = render::text(&command, "fingerprint", "");
            let record = core.records.entry(id).or_default();
            if record.generation == 0 {
                record.generation = 1;
                record.base_key = key.to_owned();
                record.target_key = key.to_owned();
                record.fingerprint = fingerprint.to_owned();
            } else if record.base_key != key || record.fingerprint != fingerprint {
                record.generation += 1;
                record.base_key = key.to_owned();
                record.fingerprint = fingerprint.to_owned();
                record.target_key = format!("{}_n{}", key, engine::hash(fingerprint));
                record.ready = false;
            }
            Ok(json!({"generation":record.generation,"ready":record.ready}))
        }
        _ => Err(format!("unknown native command: {op}")),
    }
}

fn metadata_rating(item: &Value, percent: bool) -> Value {
    let field = if percent { "critic" } else { "community" };
    if item[field].is_number() {
        return item[field].clone();
    }
    fn parse(value: &str, percent: bool) -> Option<f32> {
        let value = value.trim();
        let result = if let Some((a, b)) = value.split_once('/') {
            let a = a.trim().parse::<f32>().ok()?;
            let b = b.trim().parse::<f32>().ok()?;
            if b == 0.0 {
                return None;
            }
            a / b * if percent { 100.0 } else { 10.0 }
        } else if percent && value.ends_with('%') {
            value.trim_end_matches('%').trim().parse::<f32>().ok()?
        } else {
            let n = value.parse::<f32>().ok()?;
            if percent {
                if n <= 1.0 {
                    n * 100.0
                } else if n <= 10.0 {
                    n * 10.0
                } else {
                    n
                }
            } else {
                n
            }
        };
        result
            .is_finite()
            .then(|| result.clamp(0.0, if percent { 100.0 } else { 10.0 }))
    }
    if percent {
        if let Some(ids) = item["provider_ids"].as_object() {
            for (key, value) in ids {
                if key.to_lowercase().contains("rottentomatoes") || key.eq_ignore_ascii_case("RT") {
                    if let Some(n) = parse(value.as_str().unwrap_or(""), true) {
                        return json!(n);
                    }
                }
            }
        }
    }
    for row in item["raw_ratings"].as_array().into_iter().flatten() {
        let source = ["Source", "Name", "Key"]
            .iter()
            .find_map(|key| row[*key].as_str())
            .unwrap_or("")
            .to_lowercase();
        if (percent && source.contains("rottentomatoes"))
            || (!percent && (source.contains("imdb") || source.contains("tmdb")))
        {
            let value = ["Value", "Rating", "Score"]
                .iter()
                .find_map(|key| row[*key].as_str())
                .unwrap_or("");
            if let Some(n) = parse(value, percent) {
                return json!(n);
            }
        }
    }
    Value::Null
}

#[no_mangle]
pub extern "C" fn ei_abi_version() -> u32 {
    ABI_VERSION
}
#[no_mangle]
pub extern "C" fn ei_create() -> *mut Context {
    Box::into_raw(Box::new(Context {
        engine: Mutex::new(Engine::default()),
        ratings: ratings::Ratings::default(),
        save: Mutex::new(()),
    }))
}
#[no_mangle]
pub unsafe extern "C" fn ei_close(context: *mut Context) {
    if !context.is_null() {
        drop(Box::from_raw(context));
    }
}
#[no_mangle]
pub unsafe extern "C" fn ei_buffer_free(buffer: Buffer) {
    if !buffer.data.is_null() {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            buffer.data,
            buffer.length,
        )));
    }
}
#[no_mangle]
pub unsafe extern "C" fn ei_execute(
    context: *const Context,
    data: *const u8,
    length: usize,
    output: *mut Buffer,
) -> i32 {
    if context.is_null() || data.is_null() || output.is_null() || length > 1024 * 1024 {
        return -1;
    }
    *output = Buffer {
        data: std::ptr::null_mut(),
        length: 0,
    };
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        serde_json::from_slice(slice::from_raw_parts(data, length))
            .map_err(|e| e.to_string())
            .and_then(|c| execute(&*context, c))
    }));
    let reply = match result {
        Ok(Ok(value)) => json!({"ok":true,"value":value}),
        Ok(Err(error)) => json!({"ok":false,"value":error}),
        Err(_) => json!({"ok":false,"value":"native core panic"}),
    };
    let bytes = match serde_json::to_vec(&reply) {
        Ok(v) => v.into_boxed_slice(),
        Err(_) => return -2,
    };
    let length = bytes.len();
    *output = Buffer {
        data: Box::into_raw(bytes) as *mut u8,
        length,
    };
    0
}
#[no_mangle]
pub unsafe extern "C" fn ei_query(
    context: *const Context,
    id: u64,
    output: *mut QueryResult,
) -> i32 {
    if context.is_null() || output.is_null() {
        return -1;
    }
    *output = QueryResult {
        generation: 0,
        ready: 0,
        key_length: 0,
        key: [0; KEY_CAPACITY],
    };
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        let core = match (*context).engine.lock() {
            Ok(c) => c,
            Err(_) => return -2,
        };
        if let Some(record) = core.records.get(&id) {
            let key = record.published_key.as_bytes();
            if key.len() > KEY_CAPACITY {
                return -3;
            }
            (*output).generation = record.published_generation;
            (*output).ready = u32::from(record.ready) | (u32::from(record.legacy_path) << 1);
            (*output).key_length = key.len() as u32;
            (&mut (*output).key)[..key.len()].copy_from_slice(key);
        }
        0
    }))
    .unwrap_or(-2)
}
