use crate::{
    engine::hash,
    render::{alignment, flag, text},
};
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

pub fn read(folder: &Path) -> HashMap<u64, Value> {
    let mut rows = HashMap::new();
    for partition in 0..1024 {
        let name = folder.join(format!("{partition:03x}.json"));
        for path in [name.clone(), name.with_extension("json.bak")] {
            let Some(file) = fs::read(path)
                .ok()
                .filter(|b| b.len() <= 32 * 1024 * 1024)
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let Some(data) = file["Data"].as_str() else {
                continue;
            };
            if file["Format"] != 2 || file["Checksum"] != hash(data) {
                continue;
            }
            if let Ok(values) = serde_json::from_str::<Vec<Value>>(data) {
                let recovery = fs::read_to_string(name.with_extension("json.epoch"))
                    .ok()
                    .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()));
                for mut row in values {
                    if !["Movie", "Series", "BoxSet"].contains(&text(&row, "Kind", "")) {
                        continue;
                    }
                    if let Some(id) = row["InternalId"].as_u64().filter(|id| *id > 0) {
                        if let Some(epoch) = &recovery {
                            row["_LegacyRecovery"] = Value::String(epoch.clone());
                        }
                        rows.insert(id, row);
                    }
                }
            }
            break;
        }
    }
    rows
}

pub fn key(row: &Value, config: &Value, settings: &Value, item: &Value) -> Option<(String, bool)> {
    let binding = &row["Bindings"]["0"];
    let token = binding["Token"].as_str()?;
    let projection = binding["DataProjection"].as_str()?;
    let mut data = row["Seed"].as_str()?.to_owned();
    if let Some(epoch) = row["_LegacyRecovery"].as_str() {
        data.push('~');
        data.push_str(epoch);
    }
    if let Some(version) = row["Inputs"][projection]["Version"]
        .as_str()
        .filter(|v| !v.is_empty())
    {
        data.push('~');
        data.push_str(version);
    }
    if data == text(binding, "DataBaseline", "") {
        data = text(binding, "DataAlias", &data).to_owned();
    }
    let id = row["InternalId"].as_u64()?;
    let external = alignment(settings, "PopcornScoreIconAlignment") > 0
        || alignment(settings, "MyAnimeListScoreIconAlignment") > 0;
    let suffix = format!(
        "_r{}_{}",
        text(binding, "RatingHash", ""),
        binding["RatingVersion"].as_i64().unwrap_or(0)
    );
    let key = if flag(binding, "Fallback", false) {
        format!("ei11_i{id}_0_u{token}_{data}{suffix}")
    } else {
        format!(
            "ei11_i{id}_0_p{}_v{}_s{token}_d{data}{}",
            text(binding, "OriginalProfileId", "").replace('-', ""),
            text(config, "PersistedVersion", "1.0.0"),
            if external && !text(config, "MDBListApiKey", "").is_empty() {
                // The old non-fallback branch read these live, rather than
                // using the binding's possibly-partial metadata snapshot.
                let tmdb = item["provider_ids"]
                    .as_object()
                    .and_then(|ids| ids.iter().find(|(k, _)| k.eq_ignore_ascii_case("Tmdb")))
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("");
                let interval = if tmdb.is_empty() {
                    0
                } else {
                    let fnv = tmdb.encode_utf16().fold(14695981039346656037u64, |h, c| {
                        (h ^ c as u64).wrapping_mul(1099511628211)
                    });
                    let period = 7 * 86400 * 10_000_000u64;
                    let now = chrono::Utc::now();
                    let ticks = 621355968000000000u64
                        + now.timestamp() as u64 * 10_000_000
                        + now.timestamp_subsec_nanos() as u64 / 100;
                    (ticks + fnv % period) / period
                };
                format!("_r{}_{}", hash(text(config, "MDBListApiKey", "")), interval)
            } else {
                String::new()
            }
        )
    };
    Some((key, false)) // Legacy PNG payloads used the host's non-transparent (.jpg) cache path.
}

pub fn matches_metadata(row: &Value, item: &Value, settings: &Value) -> bool {
    // Legacy projections did not include the favorite count. Their artifacts
    // cannot establish the server-wide count required by the current renderer.
    if item["favorite_users"].as_array()
        .is_some_and(|users| users.iter().any(|v| v.as_bool() == Some(true))) {
        return false;
    }
    let mut input = String::new();
    if alignment(settings, "CommunityScoreIconAlignment") > 0 {
        input.push_str("c=");
        if let Some(v) = item["community"].as_f64() {
            input.push_str(&(v as f32).to_string());
        }
    }
    if alignment(settings, "RottenTomatoesScoreIconAlignment") > 0 {
        input.push_str("|r=");
        if let Some(v) = item["critic"].as_f64() {
            input.push_str(&(v as f32).to_string());
        }
    }
    if alignment(settings, "ParentalRatingIconAlignment") > 0 {
        input.push_str("|p=");
        input.push_str(text(item, "official_rating", ""));
    }
    if alignment(settings, "PopcornScoreIconAlignment") > 0
        || alignment(settings, "MyAnimeListScoreIconAlignment") > 0
        || (alignment(settings, "CommunityScoreIconAlignment") > 0 && item["community"].is_null())
        || (alignment(settings, "RottenTomatoesScoreIconAlignment") > 0 && item["critic"].is_null())
    {
        if let Some(ids) = item["provider_ids"].as_object() {
            let mut pairs: Vec<_> = ids.iter().collect();
            pairs.sort_by_key(|(k, _)| *k);
            for (key, value) in pairs {
                input.push('|');
                input.push_str(key);
                input.push('=');
                input.push_str(value.as_str().unwrap_or(""));
            }
        }
    }
    let projection = text(&row["Bindings"]["0"], "DataProjection", "");
    row["Inputs"][projection]["Light"] == hash(&input)
}
