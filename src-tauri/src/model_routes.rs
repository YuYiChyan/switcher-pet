//! Narrow, credential-free settings API for the new model/source selector.
use crate::{account::ModelRoutePolicy, relay_catalog, AppState};
use tauri::{Emitter, State};

#[tauri::command]
pub fn get_model_routes(state: State<AppState>) -> Result<serde_json::Value, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!(groups(&store)))
}
pub fn native_models()->Vec<String> {
    native_catalog().iter().filter_map(|m|m["slug"].as_str().or(m["id"].as_str())).map(str::to_owned).collect()
}
pub fn native_catalog()->Vec<serde_json::Value> {
    let mut models=Vec::new();
    let mut seen=std::collections::BTreeSet::new();
    for path in crate::app_paths::readonly_model_catalogs() {
        if !std::fs::metadata(&path).is_ok_and(|m|m.len()<16*1024*1024){continue;}
        let Some(value)=std::fs::read(path).ok().and_then(|b|serde_json::from_slice::<serde_json::Value>(&b).ok()) else{continue;};
        models.extend(value["models"].as_array().into_iter().flatten().filter(|row|row["slug"].as_str().or(row["id"].as_str()).is_some_and(|m|
            ((m.starts_with("gpt-")&&!m.starts_with("gpt-oss-"))||m.starts_with("codex-")||["o1","o3","o4-mini"].contains(&m))&&seen.insert(m.to_string()))).cloned());
    }
    // The desktop and CLI caches can each contain only part of the native list.
    // Merge their IDs, then match native priority with stable ties.
    models.sort_by_key(|m|m["priority"].as_i64().unwrap_or(i64::MAX));
    models
}
pub fn logical_model(requested:&str)->&str {crate::antigravity::models::gemini_family(requested.strip_prefix(relay_catalog::CURRENT_PREFIX).unwrap_or(requested))}
fn family_setting<'a,T>(map:&'a std::collections::HashMap<String,T>,model:&str)->Option<&'a T> {
    map.get(model).or_else(||model.starts_with("gemini-").then(||
        ["-high","-medium","-low","-extra-low","-tiered","-thinking"].iter().find_map(|suffix|map.get(&format!("{model}{suffix}")))).flatten())
}
pub fn route_policy<'a>(store:&'a crate::account::AccountStore,model:&str)->Option<&'a ModelRoutePolicy> {
    family_setting(&store.settings.model_route_policies,logical_model(model))
}
pub fn selected(store:&crate::account::AccountStore,requested:&str)->Option<String> {
    if requested.starts_with("relay-model:"){return None;}
    let model=logical_model(requested);
    if let Some(id)=family_setting(&store.settings.current_model_sources,model){return Some(id.clone());}
    // Plain third-party IDs now come from the same catalog as native IDs.
    // Preserve legacy relay-current behavior unless the user explicitly selected a source.
    if requested.starts_with(relay_catalog::CURRENT_PREFIX) {return None;}
    if native_models().iter().any(|m|m==model) && store.current.as_ref().is_some_and(|id|native_supports(store,id,model)){return None;}
    groups(store).iter().find(|g|g["model"]==model).and_then(|g|g["current"].as_str()).map(str::to_owned)
}
pub fn native_supports(store:&crate::account::AccountStore,id:&str,model:&str)->bool {
    store.accounts.get(id).is_some_and(|a|a.is_openai_account()) && native_models().iter().any(|m|m==model)
}
pub fn supports(store:&crate::account::AccountStore,id:&str,model:&str)->bool {
    native_supports(store,id,model)||google_supports(store,id,model)||relay_catalog::resolve_on_account(store,model,id).is_some()
}
/// Codex deserializes the entire catalog atomically. Even hidden aliases must
/// have all required fields; one incomplete row discards every third-party model.
fn complete_catalog_entry(entry:&mut serde_json::Value) {
    let id=entry["slug"].as_str().or(entry["id"].as_str()).unwrap_or_default().to_owned();
    let defaults=serde_json::json!({"slug":id,"display_name":id,"description":id,
        "default_reasoning_level":null,"supported_reasoning_levels":[],
        "shell_type":"shell_command","visibility":"list","supported_in_api":true,"priority":50,
        "base_instructions":"","default_reasoning_summary":"none","support_verbosity":false,
        "truncation_policy":{"mode":"bytes","limit":10000},"input_modalities":["text"],
        "supports_parallel_tool_calls":true,"supports_image_detail_original":false,
        "supports_search_tool":false,"experimental_supported_tools":[],"effective_context_window_percent":95});
    if let Some(object)=entry.as_object_mut() {for (key,value) in defaults.as_object().unwrap(){object.entry(key.clone()).or_insert(value.clone());}}
}
/// One public logical model per selector. Old relay slugs remain hidden aliases
/// for existing chats; source selection must not change a chat's model identity.
pub fn unify_catalog(store:&crate::account::AccountStore, entries:&mut Vec<serde_json::Value>, api_format:bool) {
    let original=entries.clone();
    let registry=groups(store);
    let native=native_catalog();
    let mut result=Vec::new();
    for (index,group) in registry.iter().enumerate() {
        let Some(logical)=group["model"].as_str() else{continue};
        if group["policy"]["enabled"]==false || group["policy"]["visible"]==false {continue;}
        let source=group["current"].as_str().unwrap_or_default();
        let relay=relay_catalog::configured_on_account(store,logical,source);
        let mut entry=if api_format {
            serde_json::json!({"id":logical,"object":"model","owned_by":"switcher-pet"})
        } else if google_supports(store,source,logical) {
            let wire=google_upstream(store,source,logical,None).unwrap_or_else(||logical.to_owned());
            crate::proxy::antigravity_codex_catalog_entry(&crate::antigravity::models::model_for_id(&wire).unwrap(),None)
        } else if let Some(ref relay)=relay {
            relay_catalog::catalog_entry(relay,None)
        } else {
            original.iter().chain(native.iter()).find(|e|e["slug"].as_str().or(e["id"].as_str())==Some(logical))
                .cloned().unwrap_or_else(||serde_json::json!({"slug":logical,"visibility":"list","supported_in_api":true}))
        };
        if !api_format {
            if relay.is_some() {
                if let Some(metadata)=native.iter().find(|e|e["slug"].as_str().or(e["id"].as_str())==Some(logical)) {
                    for key in ["supported_reasoning_levels","default_reasoning_level"] {
                        if let Some(value)=metadata.get(key){entry[key]=value.clone();}
                    }
                }
            }
            entry["slug"]=serde_json::json!(logical);
            entry["display_name"]=serde_json::json!(logical);
            entry["description"]=serde_json::json!(logical);
            entry["visibility"]=serde_json::json!("list");
            // Codex sorts by priority, not JSON array order. A selected API
            // must not move native GPT models to the relay's default rank 50.
            entry["priority"]=serde_json::json!(index+1);
            // The bridge resolves every frame to the selected source. Do not let
            // clients stick to a native upstream socket after changing providers.
            entry["prefer_websockets"]=serde_json::json!(false);
            if entry.get("id").is_some(){entry["id"]=serde_json::json!(logical);}
        }
        result.push(entry);
    }
    // Keep unrecognized native metadata and legacy hidden account pins.
    for mut entry in original {
        let Some(id)=entry["slug"].as_str().or(entry["id"].as_str()).map(str::to_owned) else{continue;};
        if registry.iter().any(|g|g["model"]==id){continue;}
        if logical_model(&id)!=id && registry.iter().any(|g|g["model"]==logical_model(&id)) {
            if api_format {continue;}
            entry["visibility"]=serde_json::json!("hide");
        }
        if relay_catalog::is_relay_model_slug(&id) {
            if api_format {continue;}
            entry["visibility"]=serde_json::json!("hide");
        }
        if !result.iter().any(|e|e["slug"].as_str().or(e["id"].as_str())==Some(&id)){result.push(entry);}
    }
    *entries=result;
    if !api_format {for entry in entries {complete_catalog_entry(entry);}}
}
pub fn groups(store:&crate::account::AccountStore)->Vec<serde_json::Value> {
    let candidates = relay_catalog::candidates(&store);
    let mut groups: Vec<_> = relay_catalog::models(&store).into_iter().filter_map(|model| {
        let policy = store.settings.model_route_policies.get(&model.upstream).cloned().unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        let sources: Vec<_> = candidates.iter().filter(|s| relay_catalog::normalized_upstream(s) == model.upstream &&
            !store.accounts.get(&s.account_id).and_then(|a|a.relay_model_map.as_ref()).is_some_and(|map|
                !map.contains_key(&model.upstream) && map.iter().any(|(logical,wire)|logical!=wire && wire==&s.upstream)) &&
            seen.insert(s.account_id.clone())).map(|source| {
            serde_json::json!({"id":source.account_id,"name":source.account_name,"protocol":source.protocol,
                "automatic_supported":source.protocol == "responses" && !source.local_agy})
        }).collect();
        // A mapped wire ID is an implementation detail of that logical model,
        // not a second selector. Keep it only if another source exposes it directly.
        if sources.is_empty(){return None;}
        let current=if sources.iter().any(|s|s["id"]==model.account_id){model.account_id}else{sources[0]["id"].as_str().unwrap_or_default().to_owned()};
        Some(serde_json::json!({"model":model.upstream,"slug":model.slug,"current":current,"policy":policy,"sources":sources}))
    }).collect();
    // Map keys are logical client model IDs; values are provider wire IDs.
    // Previously only values reached this list, making native aliases impossible
    // to select even though resolve_on_account already knew how to translate them.
    for account in store.accounts.values() {
        for logical in account.relay_model_map.iter().flat_map(|m|m.keys()) {
            if let Some(source)=relay_catalog::configured_on_account(store,logical,&account.id) {
                groups.push(serde_json::json!({"model":logical_model(logical),"slug":logical_model(logical),"current":account.id,
                    "policy":store.settings.model_route_policies.get(logical).cloned().unwrap_or_default(),
                    "sources":[{"id":source.account_id,"name":source.account_name,"protocol":source.protocol,
                        "automatic_supported":source.protocol=="responses"&&!source.local_agy}]}));
            }
        }
    }
    groups.extend(google_groups(store));
    let native=native_models();
    for model in &native {
        let sources:Vec<_>=store.accounts.values().filter(|a|a.is_openai_account()).map(|a|serde_json::json!({"id":a.id,"name":a.name,
            "protocol":if a.is_chatgpt_oauth(){"openai_subscription"}else{"openai_key"},"automatic_supported":true})).collect();
        if !sources.is_empty(){groups.push(serde_json::json!({"model":model,"slug":model,"current":store.current.clone().unwrap_or_else(||sources[0]["id"].as_str().unwrap_or_default().into()),
            "policy":store.settings.model_route_policies.get(model).cloned().unwrap_or_default(),"sources":sources}));}
    }
    let mut merged=std::collections::BTreeMap::<String,serde_json::Value>::new();
    for group in groups {
        let key=group["model"].as_str().unwrap_or_default().to_owned();
        if let Some(old)=merged.get_mut(&key) {
            let sources=old["sources"].as_array_mut().unwrap();
            for source in group["sources"].as_array().into_iter().flatten(){if !sources.iter().any(|s|s["id"]==source["id"]){sources.push(source.clone());}}
        } else {merged.insert(key,group);}
    }
    for (model,group) in &mut merged {
        if let Some(policy)=route_policy(store,model){group["policy"]=serde_json::json!(policy);}
        if let Some(id)=family_setting(&store.settings.current_model_sources,model){group["current"]=serde_json::json!(id);}
        else if native.contains(model) {
            if let Some(current)=store.current.as_ref().filter(|id|store.accounts.get(*id).is_some_and(|a|a.is_openai_account())){group["current"]=serde_json::json!(current);}
        }
        // Same logical model has one selector regardless of its protocols.
        group["slug"]=serde_json::json!(model);
        for source in group["sources"].as_array_mut().into_iter().flatten() {
            source["upstream_model"]=serde_json::json!(source["id"].as_str()
                .and_then(|id|relay_catalog::configured_on_account(store,model,id).map(|m|m.upstream).or_else(||google_upstream(store,id,model,None))).unwrap_or_else(||model.clone()));
        }
    }
    let mut ordered=Vec::new();for model in native{if let Some(group)=merged.remove(&model){ordered.push(group);}}
    let (gpt,third_party):(Vec<_>,Vec<_>)=merged.into_values().partition(|g|g["model"].as_str().is_some_and(|m|m.starts_with("gpt-")&&!m.starts_with("gpt-oss-")));
    ordered.extend(gpt);ordered.extend(third_party);ordered
}

pub fn google_supports(store:&crate::account::AccountStore,id:&str,model:&str)->bool {
    google_upstream(store,id,model,None).is_some()
}
pub fn google_upstream(store:&crate::account::AccountStore,id:&str,model:&str,effort:Option<&str>)->Option<String> {
    let a=store.accounts.get(id).filter(|a|a.is_antigravity_oauth())?;
    if !crate::antigravity::models::is_public_model_id(model){return None;}
    let quotas=a.auth_json.get("model_quotas")?.as_object()?;
    crate::antigravity::models::gemini_variant(quotas.keys().map(String::as_str),model,effort,true)
}
pub fn google_groups(store:&crate::account::AccountStore)->Vec<serde_json::Value> {
    let ids:std::collections::BTreeSet<_>=store.accounts.values().filter(|a|a.is_antigravity_oauth())
        .flat_map(|a|a.auth_json.get("model_quotas").and_then(|q|q.as_object()).into_iter().flat_map(|q|q.keys()))
        .filter(|id|crate::antigravity::models::is_public_model_id(id)).map(|id|logical_model(id).to_owned()).collect();
    ids.into_iter().map(|model|{
        let sources:Vec<_>=store.accounts.values().filter(|a|google_supports(store,&a.id,&model))
            .map(|a|serde_json::json!({"id":a.id,"name":a.name,"protocol":"google","automatic_supported":true})).collect();
        let current=store.settings.current_google_accounts.get(&model)
            .or(store.settings.current_antigravity_account_id.as_ref()).filter(|id|google_supports(store,id,&model))
            .cloned().unwrap_or_else(||sources.first().and_then(|v|v["id"].as_str()).unwrap_or_default().to_owned());
        let policy=store.settings.model_route_policies.get(&model).cloned().unwrap_or_default();
        serde_json::json!({"model":model,"slug":model,"current":current,"policy":policy,"sources":sources})
    }).collect()
}
pub fn select_source(store:&mut crate::account::AccountStore,id:&str,model:&str)->Result<(),String> {
    let model=logical_model(model);
    if google_supports(store,id,model) {
        store.settings.current_google_accounts.insert(model.into(),id.into());
    } else if !native_supports(store,id,model) {
        let resolved=relay_catalog::configured_on_account(store,model,id).ok_or("Selected source has no matching model or explicit mapping")?;
        relay_catalog::select_current(store,id,Some(relay_catalog::normalized_upstream(&resolved)))?;
    }
    store.settings.current_model_sources.insert(model.into(),id.into());Ok(())
}
#[tauri::command]
pub fn set_model_source(state:State<AppState>,app:tauri::AppHandle,id:String,model:String)->Result<(),String> {
    let mut store=state.store.lock().map_err(|e|e.to_string())?;
    let old=store.settings.clone();select_source(&mut store,&id,&model)?;
    if let Err(e)=store.save(){store.settings=old;return Err(e);}
    drop(store);let _=app.emit("accounts-updated",());Ok(())
}

#[tauri::command]
pub fn set_model_route_policy(state: State<AppState>, app: tauri::AppHandle, model: String, policy: ModelRoutePolicy) -> Result<(), String> {
    let model=logical_model(&model).to_owned();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let candidates = relay_catalog::candidates(&store);
    if !candidates.iter().any(|m| relay_catalog::normalized_upstream(m) == model || relay_catalog::configured_on_account(&store,&model,&m.account_id).is_some()) &&
        !store.accounts.keys().any(|id|google_supports(&store,id,&model)||native_supports(&store,id,&model)) { return Err("Unknown model".into()); }
    for id in &policy.allowed_sources {
        if !native_supports(&store,id,&model) && !google_supports(&store,id,&model) && !relay_catalog::configured_on_account(&store,&model,id).is_some_and(|m|m.protocol=="responses"&&!m.local_agy) {
            return Err("Automatic sources must explicitly support this exact model via Responses".into());
        }
    }
    // Merge only this policy; do not overwrite unrelated settings from stale UI state.
    let previous = store.settings.model_route_policies.insert(model.clone(), policy);
    if let Err(error) = store.save() {
        match previous { Some(value) => { store.settings.model_route_policies.insert(model, value); }, None => { store.settings.model_route_policies.remove(&model); } }
        return Err(error);
    }
    drop(store);
    let _ = app.emit("accounts-updated", ());
    Ok(())
}
