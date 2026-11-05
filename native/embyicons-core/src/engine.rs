use crate::{
    legacy,
    render::{alignment, flag, text, GROUPS},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn hash(value: &str) -> String {
    hash_bytes(value.as_bytes())
}
pub fn hash_bytes(value: &[u8]) -> String {
    Sha256::digest(value)[..16]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}
fn bucket(id: u64) -> usize {
    let mut h = 2166136261u32;
    for b in format!("i{id}").bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    h as usize % 1024
}
pub fn partition(id: u64) -> usize {
    bucket(id)
}
pub fn atomic_save(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("state has no parent")?).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    let result = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if path.exists() {
            fs::copy(path, path.with_extension("json.bak")).map_err(|e| e.to_string())?;
        }
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    pub base_key: String,
    pub fingerprint: String,
    pub target_key: String,
    pub generation: u64,
    #[serde(skip)]
    pub running: Option<u64>,
    pub published_key: String,
    pub published_generation: u64,
    pub ready: bool,
    pub recovery: u64,
    pub path: String,
    pub legacy_path: bool,
    pub settings: Value,
    pub data: Value,
    pub item: Value,
    pub profile_id: String,
    pub source_stamp: String,
    pub source_hash: String,
    pub event_seq: u64,
    pub inspected_seq: u64,
    pub review_at: u64,
    pub due: u64,
    pub attempts: u32,
    pub error: String,
    pub waiting_input: bool,
    #[serde(skip)]
    pub inspecting: bool,
    #[serde(skip)]
    pub automatic: bool,
    #[serde(skip)]
    pub rendered: bool,
}
impl Record {
    pub fn key(&self) -> String {
        if self.recovery == 0 {
            self.target_key.clone()
        } else {
            format!("{}_c{}", self.target_key, self.recovery)
        }
    }
}

#[derive(Default)]
pub struct Engine {
    pub records: HashMap<u64, Record>,
    pub config: Value,
    pub libraries: Value,
    pub folder: PathBuf,
    pub cache: PathBuf,
    pub legacy: HashMap<u64, Value>,
    pending: VecDeque<u64>,
    queued: HashSet<u64>,
    pub dirty: HashSet<usize>,
    maintenance: Vec<u64>,
    cursor: usize,
    pub reconcile: bool,
    pub drawn: u64,
    pub reused: u64,
    pub draw_ms: u64,
    pub render_calls: u64,
    batches: HashMap<String, HashMap<u64, (u64, u64)>>,
    deadlines: HashMap<String, u64>,
    _lock: Option<fs::File>,
    pub pilot: HashSet<u64>,
    pub last_manual: Value,
}
impl Engine {
    pub fn initialize(&mut self, command: &Value) -> Result<Value, String> {
        if self._lock.is_some() {
            return Err("native core already initialized".into());
        }
        self.configure(&command["config"], &command["libraries"])?;
        self.pilot = command["pilot"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_u64())
            .collect();
        let data = PathBuf::from(text(command, "data_path", ""));
        self.cache = PathBuf::from(text(command, "cache_path", ""));
        if !data.is_absolute() || !self.cache.is_absolute() {
            return Err("absolute host data/cache paths required".into());
        }
        self.folder = data.join("rust-state-v1");
        fs::create_dir_all(&self.folder).map_err(|e| e.to_string())?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.folder.join("writer.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "another native state writer is active")?;
        self._lock = Some(lock);
        for b in 0..1024 {
            let path = self.folder.join(format!("{b:03x}.json"));
            let mut found = false;
            for candidate in [path.clone(), path.with_extension("json.bak")] {
                let Some(envelope) = fs::read(&candidate)
                    .ok()
                    .filter(|v| v.len() <= 32 * 1024 * 1024)
                    .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
                else {
                    continue;
                };
                let Some(data) = envelope["Data"].as_str() else {
                    continue;
                };
                if envelope["Format"] != 1 || envelope["Checksum"] != hash(data) {
                    continue;
                }
                if let Ok(rows) = serde_json::from_str::<HashMap<u64, Record>>(data) {
                    if rows.keys().all(|id| bucket(*id) == b) {
                        self.records.extend(rows);
                        found = true;
                        if candidate != path {
                            self.dirty.insert(b);
                        }
                        break;
                    }
                }
            }
            if !found && path.exists() {
                self.dirty.insert(b);
            }
        }
        self.maintenance = self.records.keys().copied().collect();
        self.legacy = legacy::read(&data.join("image-state-v2"));
        self.reconcile = true;
        Ok(json!({"loaded":self.records.len(),"legacy":self.legacy.len(),"abi":2}))
    }

    pub fn configure(&mut self, config: &Value, libraries: &Value) -> Result<Value, String> {
        validate_config(config)?;
        self.config = config.clone();
        self.libraries = libraries.clone();
        self.reconcile = true;
        for record in self.records.values_mut() {
            record.event_seq = record.event_seq.saturating_add(1);
            if record.running.is_some() {
                record.generation = record.generation.saturating_add(1);
            }
        }
        Ok(json!({"ok":true}))
    }
    pub fn profile(&self, item: &Value) -> Option<Value> {
        if !self.pilot.is_empty() && !self.pilot.contains(&item["id"].as_u64()?) {
            return None;
        }
        let mappings = self.config["LibraryProfileMappings"].as_array()?;
        let direct = if text(item, "kind", "") == "BoxSet" {
            item["parent_id"].as_u64().map(|i| i.to_string())
        } else {
            None
        };
        let path = if text(item, "kind", "") == "BoxSet" {
            if !flag(&self.config, "EnableCollectionProfileLookup", true)
                && !mappings
                    .iter()
                    .any(|m| Some(text(m, "LibraryId", "")) == direct.as_deref())
            {
                return None;
            }
            text(item, "member_path", "")
        } else {
            text(item, "path", "")
        };
        let library = self
            .libraries
            .as_array()?
            .iter()
            .filter_map(|lib| {
                let id = lib["id"].as_str()?;
                if direct.as_deref() == Some(id) {
                    return Some((usize::MAX, id));
                }
                lib["paths"]
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|prefix| {
                        !prefix.is_empty()
                            && (path == *prefix
                                || path
                                    .strip_prefix(*prefix)
                                    .is_some_and(|s| s.starts_with('/') || s.starts_with('\\')))
                    })
                    .map(|p| (p.len(), id))
                    .max_by_key(|p| p.0)
            })
            .max_by_key(|p| p.0)?
            .1;
        let profile_id = mappings
            .iter()
            .find(|m| text(m, "LibraryId", "") == library)?["ProfileId"]
            .as_str()?;
        self.config["Profiles"]
            .as_array()?
            .iter()
            .find(|p| text(p, "Id", "").eq_ignore_ascii_case(profile_id))
            .cloned()
    }
    pub fn enqueue(&mut self, id: u64, automatic: bool) -> bool {
        if id == 0 {
            return true;
        }
        if !self.records.contains_key(&id) {
            self.maintenance.push(id);
        }
        let record = self.records.entry(id).or_default();
        record.event_seq = record.event_seq.saturating_add(1);
        record.automatic |= automatic;
        record.attempts = 0;
        record.due = now_millis();
        self.push(id)
    }
    fn push(&mut self, id: u64) -> bool {
        if self.queued.contains(&id) {
            return true;
        }
        if self.pending.len() >= 4096 {
            self.reconcile = true;
            return false;
        }
        self.queued.insert(id);
        self.pending.push_back(id);
        true
    }
    pub fn next(&mut self) -> Value {
        let now = now_millis();
        for _ in 0..self.pending.len() {
            let id = self.pending.pop_front().unwrap();
            self.queued.remove(&id);
            let Some(record) = self.records.get_mut(&id) else {
                continue;
            };
            if record.inspecting || record.running.is_some() || record.due > now {
                self.push(id);
                continue;
            }
            record.inspecting = true;
            return json!({"id":id,"sequence":record.event_seq});
        }
        Value::Null
    }
    pub fn observed(
        &mut self,
        id: u64,
        sequence: u64,
        item: Value,
        profile: Option<Value>,
        data: Value,
        source_stamp: String,
        source_hash: String,
    ) -> Result<Value, String> {
        let Some(profile) = profile else {
            self.records.remove(&id);
            self.dirty.insert(bucket(id));
            return Ok(json!({"skip":true}));
        };
        let settings = profile["Settings"].clone();
        if !supported(&item, &settings) {
            self.records.remove(&id);
            self.dirty.insert(bucket(id));
            return Ok(json!({"skip":true}));
        }
        let mut relevant = serde_json::Map::new();
        let visual = crate::render::visual_data(&settings, &data);
        if visual.as_object().is_some_and(|groups| {
            groups
                .values()
                .any(|group| !text(group, "label", "").is_empty())
        }) {
            relevant.insert(
                "font_revision".into(),
                json!(crate::render::font_revision()),
            );
        }
        for group in GROUPS {
            if visual[group].is_null() {
                continue;
            }
            for (key, value) in settings
                .as_object()
                .ok_or("profile settings must be an object")?
            {
                let shape = &settings[format!("{group}BackgroundShape")];
                let no_background = shape == 0 || shape == "None" || shape.is_null();
                if key.starts_with(group)
                    && !(no_background
                        && (key.ends_with("BackgroundColor") || key.ends_with("BackgroundOpacity")))
                {
                    relevant.insert(key.clone(), value.clone());
                }
            }
        }
        for key in [
            "IconSize",
            "IconSpacing",
            "EdgePaddingHorizontal",
            "EdgePaddingVertical",
            "RatingFontSizeMultiplier",
            "RatingTextVerticalOffset",
            "RatingPercentageSuffix",
        ] {
            relevant.insert(key.to_owned(), settings[key].clone());
        }
        for corner in ["TopLeft", "TopRight", "BottomLeft", "BottomRight"] {
            for suffix in ["IconSize", "HorizontalPadding", "VerticalPadding"] {
                let key = format!("{corner}{suffix}");
                relevant.insert(key.clone(), settings[&key].clone());
            }
        }
        relevant.insert(
            "smoothing".into(),
            self.config["EnableImageSmoothing"].clone(),
        );
        let render_settings = Value::Object(relevant);
        let fingerprint = hash(
            &json!({"settings":render_settings,"data":visual,"source":source_hash}).to_string(),
        );
        let record = self.records.entry(id).or_default();
        record.inspecting = false;
        record.inspected_seq = sequence;
        if record.event_seq != sequence {
            self.push(id);
            return Ok(json!({"superseded":true}));
        }
        let host_changed = record.generation > 0
            && (record.item["image_ticks"] != item["image_ticks"]
                || record.item["original"] != item["original"]);
        if record.generation == 0 {
            record.generation = 1;
            record.base_key = format!("ei_r1_i{id}_{fingerprint}");
            record.target_key = record.base_key.clone();
            if let Some(row) = self.legacy.remove(&id) {
                if legacy::matches_metadata(&row, &item, &settings) {
                    if let Some((key, _)) = legacy::key(&row, &self.config, &settings, &item) {
                        record.base_key = key.clone();
                        record.target_key = key;
                        record.legacy_path = true;
                    }
                }
            }
        } else if record.fingerprint != fingerprint {
            record.generation = record
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            record.target_key = format!("ei_r1_i{id}_{fingerprint}");
            record.legacy_path = false;
            record.ready = false;
            record.recovery = 0;
        } else if host_changed {
            record.generation = record
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            record.ready = false;
        }
        record.fingerprint = fingerprint;
        record.settings = render_settings;
        record.data = data;
        record.item = item;
        record.profile_id = text(&profile, "Id", "").to_owned();
        record.source_hash = source_hash;
        record.source_stamp = source_stamp;
        record.review_at = now_millis() + 3600_000;
        record.waiting_input = false;
        self.dirty.insert(bucket(id));
        self.begin(id)
    }
    pub fn begin(&mut self, id: u64) -> Result<Value, String> {
        let record = self.records.get_mut(&id).ok_or("item was not staged")?;
        if record.ready || record.running.is_some() {
            return Ok(Value::Null);
        }
        record.running = Some(record.generation);
        record.rendered = false;
        Ok(
            json!({"id":id,"generation":record.generation,"key":record.key(),"legacy_path":record.legacy_path,"original":record.item["original"]}),
        )
    }
    pub fn commit(&mut self, id: u64, generation: u64, path: &str) -> Result<Value, String> {
        let record = self.records.get_mut(&id).ok_or("item was removed")?;
        if record.running != Some(generation) {
            return Err("job does not own this item".into());
        }
        record.running = None;
        if record.generation != generation
            || record.event_seq != record.inspected_seq
            || path.is_empty()
        {
            self.push(id);
            return Ok(json!({"published":false}));
        }
        record.published_key = record.key();
        record.published_generation = generation;
        record.path = path.to_owned();
        record.ready = true;
        record.error.clear();
        record.attempts = 0;
        record.automatic = false;
        if record.rendered {
            self.drawn += 1;
        } else {
            self.reused += 1;
        }
        self.dirty.insert(bucket(id));
        Ok(json!({"published":true,"rendered":record.rendered}))
    }
    pub fn missing(&mut self, id: u64, generation: u64) -> Result<Value, String> {
        let record = self.records.get_mut(&id).ok_or("item was removed")?;
        if record.ready && record.published_generation == generation {
            record.generation = record
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            record.recovery = record.recovery.checked_add(1).ok_or("recovery exhausted")?;
            record.ready = false;
            self.dirty.insert(bucket(id));
        }
        Ok(json!({"ready":record.ready,"generation":record.generation}))
    }
    pub fn failed(&mut self, id: u64, generation: u64, message: &str) -> Value {
        if let Some(record) = self.records.get_mut(&id) {
            record.inspecting = false;
            record.waiting_input = false;
            if record.running == Some(generation) {
                record.running = None;
                record.generation = record.generation.saturating_add(1);
            }
            record.attempts = record.attempts.saturating_add(1);
            record.error = message.chars().take(256).collect();
            record.due = now_millis()
                + [5000, 30000, 120000, 600000, 3600000]
                    [(record.attempts as usize).saturating_sub(1).min(4)];
            if record.attempts <= 5 {
                self.push(id);
            }
            self.dirty.insert(bucket(id));
        }
        Value::Null
    }
    pub fn maintenance_snapshot(&mut self) -> Vec<(u64, u64, bool, String)> {
        let mut result = Vec::new();
        let len = self.maintenance.len();
        for _ in 0..len.min(512) {
            let id = self.maintenance[self.cursor % len];
            self.cursor = (self.cursor + 1) % len;
            if let Some(record) = self.records.get(&id) {
                result.push((
                    id,
                    record.published_generation,
                    record.ready,
                    record.path.clone(),
                ));
            }
        }
        result
    }
    pub fn maintenance_result(&mut self, id: u64, generation: u64, missing: bool) {
        if missing {
            let _ = self.missing(id, generation);
        }
        if let Some(record) = self.records.get(&id) {
            if record.attempts <= 5 && (!record.ready || record.review_at <= now_millis()) {
                self.push(id);
            }
        }
    }
    pub fn stats(&self) -> Value {
        json!({"Backend":"Rust","Abi":2,"Tracked":self.records.len(),"Ready":self.records.values().filter(|r|r.ready).count(),"Pending":self.pending.len(),"Running":self.records.values().filter(|r|r.running.is_some()||r.inspecting).count(),"Drawn":self.drawn,"RenderCalls":self.render_calls,"Reused":self.reused,"DrawMilliseconds":self.draw_ms,"Failed":self.records.values().filter(|r|!r.error.is_empty() && !r.waiting_input).count(),"WaitingInput":self.records.values().filter(|r|r.waiting_input).count(),"NeedsReconcile":self.reconcile,"PilotIds":self.pilot,"LastManual":self.last_manual})
    }
    pub fn prioritize(&mut self, id: u64) {
        if self.queued.contains(&id) {
            self.pending.retain(|item| *item != id);
            self.pending.push_front(id);
        }
    }
    pub fn scan_backpressure(&self) -> bool {
        self.pending.len() > 3500
    }
    pub fn notify(&mut self, command: &Value) -> Value {
        let batch = text(command, "batch", "").to_owned();
        let automatic = flag(command, "automatic", true);
        let mut full = false;
        if let Some(ids) = command["ids"].as_array() {
            for id in ids.iter().filter_map(|v| v.as_u64()) {
                if !self.enqueue(id, automatic) {
                    full = true;
                }
                if !batch.is_empty() {
                    self.deadlines
                        .entry(batch.clone())
                        .or_insert_with(|| now_millis() + 120_000);
                    let record = &self.records[&id];
                    let target = (record.event_seq, record.published_generation);
                    self.batches
                        .entry(batch.clone())
                        .or_default()
                        .insert(id, target);
                }
            }
        }
        json!({"backpressure":full,"pending":self.pending.len()})
    }
    pub fn batch_status(&self, batch: &str) -> Value {
        let Some(targets) = self.batches.get(batch) else {
            return json!({"total":0,"complete":0,"deferred":0,"failed":0,"pending":0,"finished":true});
        };
        let (mut complete, mut deferred, mut failed, mut pending) = (0, 0, 0, 0);
        let (mut drawn, mut reused) = (0, 0);
        for (id, (seq, baseline)) in targets {
            match self.records.get(id) {
                None => deferred += 1,
                Some(r) => {
                    if r.inspected_seq >= *seq && r.ready {
                        complete += 1;
                        if r.published_generation > *baseline && r.rendered {
                            drawn += 1;
                        } else {
                            reused += 1;
                        }
                    } else if r.waiting_input && r.attempts <= 5 {
                        deferred += 1;
                    } else if r.attempts > 0 {
                        failed += 1;
                    } else if r.event_seq > *seq && r.inspected_seq >= *seq {
                        deferred += 1;
                    } else {
                        pending += 1;
                    }
                }
            }
        }
        json!({"total":targets.len(),"complete":complete,"drawn":drawn,"reused":reused,"deferred":deferred,"failed":failed,"pending":pending,"finished":pending==0||self.deadlines.get(batch).is_some_and(|deadline|now_millis()>=*deadline)})
    }
    pub fn batch_end(&mut self, batch: &str, cancel: bool) -> Value {
        self.deadlines.remove(batch);
        if let Some(targets) = self.batches.remove(batch) {
            if !cancel {
                return Value::Null;
            }
            for id in targets.keys() {
                if self
                    .records
                    .get(id)
                    .is_some_and(|r| !r.automatic && !r.inspecting && r.running.is_none())
                {
                    self.queued.remove(id);
                    self.pending.retain(|i| i != id);
                    if self
                        .records
                        .get(id)
                        .is_some_and(|r| r.published_key.is_empty())
                    {
                        self.records.remove(id);
                        self.dirty.insert(bucket(*id));
                    }
                }
            }
        }
        Value::Null
    }
    pub fn save_snapshot(&mut self) -> Vec<(usize, PathBuf, Vec<u8>)> {
        let dirty: Vec<_> = self.dirty.drain().collect();
        dirty
            .into_iter()
            .map(|b| {
                let rows: HashMap<_, _> = self
                    .records
                    .iter()
                    .filter(|(id, _)| bucket(**id) == b)
                    .collect();
                let data = serde_json::to_string(&rows).unwrap();
                let bytes =
                    serde_json::to_vec(&json!({"Format":1,"Checksum":hash(&data),"Data":data}))
                        .unwrap();
                (b, self.folder.join(format!("{b:03x}.json")), bytes)
            })
            .collect()
    }
}

pub fn supported(item: &Value, settings: &Value) -> bool {
    let kind = text(item, "kind", "");
    if !["Movie", "Series", "BoxSet"].contains(&kind) || !flag(settings, "EnableForPosters", true) {
        return false;
    }
    if kind == "BoxSet" && !flag(settings, "UseCollectionLiteMode", true) {
        return false;
    }
    GROUPS
        .iter()
        .any(|g| alignment(settings, &format!("{g}IconAlignment")) > 0)
}
pub fn validate_config(config: &Value) -> Result<(), String> {
    let profiles = config["Profiles"]
        .as_array()
        .ok_or("configuration Profiles must be an array")?;
    for profile in profiles {
        let settings = &profile["Settings"];
        if flag(settings, "EnableForThumbs", false)
            || flag(settings, "EnableForBanners", false)
            || flag(settings, "NormalizePosterAspectRatio", false)
            || flag(settings, "EnableTopIconBar", false)
            || flag(settings, "EnableBottomIconBar", false)
        {
            return Err(
                "this build supports original-size Primary posters without normalization or bars"
                    .into(),
            );
        }
        if let Some(fields) = settings.as_object() {
            for (key, value) in fields {
                if key.ends_with("IconAlignment")
                    && !GROUPS.iter().any(|g| key == &format!("{g}IconAlignment"))
                    && value != 0
                    && value != "Disabled"
                {
                    return Err(format!("unsupported enabled overlay: {key}"));
                }
            }
        } else {
            return Err("profile Settings must be an object".into());
        }
        for key in ["FilenameBasedIcons", "TagBasedIcons"] {
            if settings[key]
                .as_array()
                .is_some_and(|values| values.iter().any(|v| alignment(v, "IconAlignment") > 0))
            {
                return Err("filename/tag rules are outside this build".into());
            }
        }
    }
    if !text(config, "IconsFolder", "").is_empty() {
        return Err("custom icon folders are outside this build".into());
    }
    Ok(())
}

pub fn source(item: &Value, record: Option<&Record>) -> Result<(String, String), String> {
    let path = text(item, "original", "");
    let metadata = fs::metadata(path).map_err(|e| format!("original artwork unavailable: {e}"))?;
    let stamp = format!(
        "{path}|{}|{}",
        metadata.len(),
        metadata
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    if let Some(record) = record {
        if record.source_stamp == stamp && !record.source_hash.is_empty() {
            return Ok((stamp, record.source_hash.clone()));
        }
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    Ok((stamp, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_failures_exhaust_retries_without_being_reset_by_observation() {
        let mut core = Engine::default();
        let profile = json!({"Id":"test", "Settings":{"CommunityScoreIconAlignment":1}});
        core.enqueue(1, true);
        for attempt in 1..=6 {
            let next = core.next();
            assert_eq!(next["id"], 1);
            let job = core
                .observed(
                    1,
                    next["sequence"].as_u64().unwrap(),
                    json!({"kind":"Movie"}),
                    Some(profile.clone()),
                    json!({"community":8.1}),
                    "stamp".into(),
                    "hash".into(),
                )
                .unwrap();
            core.failed(1, job["generation"].as_u64().unwrap(), "encoder failure");
            assert_eq!(core.records[&1].attempts, attempt);
            core.records.get_mut(&1).unwrap().due = 0;
        }
        core.maintenance_result(1, 0, false);
        assert!(core.next().is_null());
    }
}
