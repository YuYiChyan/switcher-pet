//! Read-only model metadata observer. Never reads auth/config or writes Codex files.
//! Focus is not available: rank main chats by latest turn_context, not tool/log mtime.
use std::{collections::HashMap, fs, io::{BufRead, BufReader, Read}, path::{Path,PathBuf}, time::{Duration,SystemTime}};
use tauri::Manager;
use serde_json::Value;

fn collect(root:&Path, depth:usize, out:&mut Vec<(SystemTime,PathBuf)>) {
    if depth>4 || out.len()>20000{return;}
    let Ok(entries)=fs::read_dir(root) else{return;};
    for entry in entries.flatten() {
        let Ok(meta)=entry.path().symlink_metadata() else{continue;};
        if meta.file_type().is_symlink(){continue;}
        if meta.is_dir(){collect(&entry.path(),depth+1,out);}
        else if entry.path().extension().is_some_and(|e|e=="jsonl") {
            if let Ok(mtime)=meta.modified(){out.push((mtime,entry.path()));}
        }
    }
}
fn scan(path:&Path)->Option<crate::pet::Activity> {
    // Bounded memory per line and no copied chat contents in state/logs.
    let file=fs::File::open(path).ok()?;
    if file.metadata().ok()?.len()>64*1024*1024 {return None;}
    let mut reader=BufReader::new(file);let mut buf=Vec::new();let mut session=String::new();let mut best=None;
    loop {
        buf.clear();let n=reader.by_ref().take(2*1024*1024).read_until(b'\n',&mut buf).ok()?;
        if n==0{break;}
        if buf.last()!=Some(&b'\n') {
            // Skip an oversized record without allocating its remainder; a partial
            // final record is re-read when the file grows on the next scan.
            loop {let available=reader.fill_buf().ok()?;if available.is_empty(){break;}
                let end=available.iter().position(|b|*b==b'\n');let count=end.map(|i|i+1).unwrap_or(available.len());reader.consume(count);if end.is_some(){break;}}
            continue;
        }
        let Ok(value)=serde_json::from_slice::<Value>(&buf) else{continue;};
        if value["type"]=="session_meta" {
            let source=&value["payload"]["source"];
            if source.is_object() || source.as_str().is_some_and(|s|s.contains("subagent")){return None;}
            session=value["payload"]["id"].as_str()?.to_owned();
        }
        if value["type"]=="turn_context" && !session.is_empty() {
            let Some(model)=value["payload"]["model"].as_str() else{continue;};
            let Some(time)=value["timestamp"].as_str().and_then(|t|chrono::DateTime::parse_from_rfc3339(t).ok()) else{continue;};
            best=Some(crate::pet::Activity {model:model.to_owned(),source:"Codex 主对话（只读观察）".into(),account_id:String::new(),at:time.timestamp(),session_id:session.clone(),origin:"codex".into()});
        }
    }
    best
}
pub fn start(app:tauri::AppHandle) {
    let Some(root)=crate::app_paths::readonly_codex_sessions_root() else{return;};
    // Blocking disk I/O runs off the UI and async reactor threads.
    std::thread::spawn(move || {
        let mut cache=HashMap::<PathBuf,(SystemTime,Option<crate::pet::Activity>)>::new();
        loop {
            let enabled=app.state::<crate::AppState>().store.lock().map(|s|s.settings.pet_preferences.follow_activity).unwrap_or(false);
            if enabled {
                let mut files=Vec::new();collect(&root,0,&mut files);files.sort_by(|a,b|b.0.cmp(&a.0));files.truncate(16);
                cache.retain(|path,_|files.iter().any(|(_,p)|p==path));
                for (mtime,path) in files {
                    if cache.get(&path).is_none_or(|(old,_)|*old!=mtime) {let value=scan(&path);cache.insert(path,(mtime,value));}
                }
                if let Some(value)=cache.values().filter_map(|(_,a)|a.as_ref()).max_by_key(|a|a.at) {
                    crate::pet::observe(&app,value.clone());
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}
