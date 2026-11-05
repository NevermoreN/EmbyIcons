use crate::{
    engine::{validate_config, Engine, Record},
    render::{self, flag, text},
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

fn profile_data(command: &Value) -> Result<Value, String> {
    let input =
        serde_json::from_str::<Value>(text(command, "json", "")).map_err(|e| e.to_string())?;
    if !input["Version"].is_string() || !input["Profiles"].is_array() {
        return Err("profile export Version/Profiles are required".into());
    }
    for profile in input["Profiles"].as_array().unwrap() {
        let name = text(profile, "Name", "");
        if name.trim().is_empty() || name.chars().count() > 256 {
            return Err("profile name must contain 1–256 characters".into());
        }
        validate_config(&json!({"Profiles":[profile]}))?;
    }
    Ok(input)
}
fn info(id: u64, r: &Record) -> Value {
    json!({"Id":id.to_string(),"Name":r.item["name"],"Type":r.item["kind"],"Path":r.item["path"]})
}
pub fn run(core: &mut Engine, command: &Value) -> Result<Value, String> {
    match text(command, "action", "") {
        "validate_import" => match profile_data(command) {
            Ok(input) => Ok(
                json!({"IsValid":true,"Errors":[],"Warnings":[],"ProfileCount":input["Profiles"].as_array().unwrap().len(),"Version":input["Version"]}),
            ),
            Err(error) => {
                Ok(json!({"IsValid":false,"Errors":[error],"Warnings":[],"ProfileCount":0}))
            }
        },
        "export" => {
            let requested = text(command, "ids", "");
            let ids: Vec<_> = requested.split(',').collect();
            let profiles: Vec<_> = core.config["Profiles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| {
                    requested.is_empty()
                        || ids
                            .iter()
                            .any(|id| id.eq_ignore_ascii_case(text(p, "Id", "")))
                })
                .map(|p| json!({"Name":p["Name"],"Settings":p["Settings"]}))
                .collect();
            let mut data =
                json!({"Version":"1.0","ExportDate":Utc::now().to_rfc3339(),"Profiles":profiles});
            if flag(command, "mappings", false) {
                let names: HashMap<_, _> = core.config["Profiles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| (text(p, "Id", ""), p["Name"].clone()))
                    .collect();
                data["LibraryMappings"] = json!(core.config["LibraryProfileMappings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| names
                        .get(text(m, "ProfileId", ""))
                        .map(|name| json!({"LibraryId":m["LibraryId"],"ProfileName":name})))
                    .collect::<Vec<_>>());
            }
            Ok(
                json!({"Success":true,"ProfileCount":profiles.len(),"JsonData":serde_json::to_string_pretty(&data).map_err(|e|e.to_string())?}),
            )
        }
        "import" => {
            let input = profile_data(command)?;
            let mut config = core.config.clone();
            let profiles = config["Profiles"]
                .as_array_mut()
                .ok_or("invalid current profiles")?;
            let mut imported = Vec::new();
            let mut updated = Vec::new();
            let mut skipped = Vec::new();
            let mut name_to_id = HashMap::new();
            for profile in input["Profiles"].as_array().unwrap() {
                let mut name = text(profile, "Name", "").trim().to_owned();
                let original = name.clone();
                let existing = profiles
                    .iter()
                    .position(|p| text(p, "Name", "").eq_ignore_ascii_case(&name));
                if let Some(index) = existing {
                    if flag(command, "overwrite", false) {
                        profiles[index]["Settings"] = profile["Settings"].clone();
                        name_to_id.insert(original, profiles[index]["Id"].clone());
                        updated.push(name);
                        continue;
                    }
                    if !flag(command, "rename", true) {
                        skipped.push(name);
                        continue;
                    }
                    let mut suffix = 2;
                    while profiles
                        .iter()
                        .any(|p| text(p, "Name", "").eq_ignore_ascii_case(&name))
                    {
                        name = format!("{original} ({suffix})");
                        suffix += 1;
                    }
                }
                let id = uuid::Uuid::new_v4().to_string();
                profiles.push(json!({"Id":id,"Name":name,"Settings":profile["Settings"]}));
                name_to_id.insert(original, json!(id));
                imported.push(name);
            }
            let mut mapping_count = 0;
            let mut skipped_mappings = Vec::new();
            if flag(command, "mappings", true) {
                for mapping in input["LibraryMappings"].as_array().into_iter().flatten() {
                    let library = text(mapping, "LibraryId", "");
                    let name = text(mapping, "ProfileName", "");
                    if !core
                        .libraries
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|l| text(l, "id", "") == library)
                        || !name_to_id.contains_key(name)
                    {
                        skipped_mappings.push(library.to_owned());
                        continue;
                    }
                    let mappings = config["LibraryProfileMappings"]
                        .as_array_mut()
                        .ok_or("invalid current library mappings")?;
                    if let Some(old) = mappings
                        .iter_mut()
                        .find(|m| text(m, "LibraryId", "") == library)
                    {
                        old["ProfileId"] = name_to_id[name].clone();
                    } else {
                        mappings.push(json!({"LibraryId":library,"ProfileId":name_to_id[name]}));
                    }
                    mapping_count += 1;
                }
            }
            validate_config(&config)?;
            Ok(
                json!({"config":config,"response":{"Success":true,"ImportedCount":imported.len(),"UpdatedCount":updated.len(),"SkippedCount":skipped.len(),"FailedCount":0,"ImportedProfiles":imported,"UpdatedProfiles":updated,"SkippedProfiles":skipped,"FailedProfiles":[],"ImportedLibraryMappingCount":mapping_count,"SkippedLibraryMappings":skipped_mappings}}),
            )
        }
        "default_profile" => {
            let mut profile = command["template"].clone();
            profile["Id"] = json!(uuid::Uuid::new_v4().to_string());
            profile["Name"] = json!("New Profile");
            for (key, value) in profile["Settings"]
                .as_object_mut()
                .ok_or("invalid profile template")?
            {
                if key.ends_with("IconAlignment") {
                    *value = json!(0);
                }
            }
            profile["Settings"]["CommunityScoreIconAlignment"] = json!(1);
            profile["Settings"]["ParentalRatingIconAlignment"] = json!(3);
            Ok(profile)
        }
        "refresh" => {
            core.reconcile = true;
            Ok(
                json!({"Success":true,"Message":"Background input verification requested; existing valid pictures are retained."}),
            )
        }
        "icon_report" => {
            let used: BTreeSet<_> = core
                .records
                .values()
                .flat_map(|r| render::used_icons(&r.data))
                .collect();
            let parental = ["g", "pg", "pg-13", "r", "nc-17", "tv-14", "tv-ma", "tv-pg"];
            let ratings = [
                "imdb", "tomato", "splat", "popcorn", "spilled", "fresh", "mal", "heart",
            ];
            let mut counts = serde_json::Map::new();
            for kind in ["Movie", "Series", "BoxSet"] {
                counts.insert(
                    kind.to_owned(),
                    json!(core
                        .records
                        .values()
                        .filter(|r| text(&r.item, "kind", "") == kind)
                        .count()),
                );
            }
            Ok(
                json!({"Groups":{"ParentalRating":{"FoundInLibrary":used.iter().filter(|s|parental.contains(&s.as_str())).collect::<Vec<_>>(),"FoundInFolder":parental},"CommunityRating":{"FoundInLibrary":used.iter().filter(|s|ratings.contains(&s.as_str())).collect::<Vec<_>>(),"FoundInFolder":ratings}},"Statistics":{"TotalItems":core.records.len(),"PosterCounts":counts},"ReportDate":Utc::now().to_rfc3339(),"Cache":core.stats()}),
            )
        }
        "items" => {
            let icon = text(command, "icon", "").to_lowercase();
            let limit = command["limit"].as_u64().unwrap_or(100).min(500) as usize;
            let items: Vec<_> = core
                .records
                .iter()
                .filter(|(_, r)| render::used_icons(&r.data).contains(&icon))
                .map(|(id, r)| info(*id, r))
                .collect();
            Ok(
                json!({"TotalCount":items.len(),"Items":items.into_iter().take(limit).collect::<Vec<_>>()}),
            )
        }
        "diagnostic" => {
            let kind = text(command, "kind", "Movie");
            let requested = text(command, "id", "");
            let items: Vec<_> = core
                .records
                .iter()
                .filter(|(id, r)| {
                    text(&r.item, "kind", "") == kind
                        && (requested.is_empty()
                            || requested == id.to_string()
                            || requested.replace('-', "") == text(&r.item, "guid", ""))
                })
                .collect();
            if kind == "Series" {
                return Ok(json!(items.iter().map(|(id,r)|json!({"SeriesId":id.to_string(),"SeriesName":r.item["name"],"TotalEpisodes":0,"Scope":"Primary poster","Checks":[{"CheckName":"Poster","Status":if r.ready{"OK"}else{"Warning"},"Message":if r.ready{"Saved in Emby cache"}else{&r.error},"DominantValues":render::used_icons(&r.data),"MismatchedEpisodes":[]}],"CacheKey":r.published_key})).collect::<Vec<_>>()));
            }
            let checks=items.first().map(|(_,r)|render::used_icons(&r.data).iter().map(|name|json!({"CheckName":name,"Value":name,"HasIcon":true,"Status":"OK"})).collect::<Vec<_>>()).unwrap_or_default();
            Ok(
                json!({"IsSingleMovieScan":!requested.is_empty(),"TotalMoviesScanned":items.len(),"MovieName":items.first().map(|(_,r)|r.item["name"].clone()).unwrap_or(Value::Null),"MovieId":requested,"Checks":checks,"LibraryGroups":[],"Cache":core.stats()}),
            )
        }
        "progress" => Ok(
            json!({"Current":core.records.values().filter(|r|r.ready).count(),"Total":core.records.len(),"Message":"Rust background poster generation","IsComplete":core.stats()["Pending"]==0 && core.stats()["Running"]==0}),
        ),
        "template_stats" => {
            Ok(json!({"Backend":"Rust","Count":render::asset_count(),"Cache":core.stats()}))
        }
        "clear_templates" => {
            render::clear_assets();
            Ok(json!({"Success":true}))
        }
        _ => Err("unknown management action".into()),
    }
}
