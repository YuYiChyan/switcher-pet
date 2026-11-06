//! Fork-owned desktop preferences and local (never bundled) sound files.
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use base64::Engine;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub silent_start: bool,
    pub pinch: bool,
    pub follow_activity: bool,
    pub fixed_family: String,
    pub muted: bool,
    pub total: bool,
    pub volume: f64,
    pub reduce_motion: bool,
    pub detailed: bool,
    pub menu_scope: String,
    pub menu_weekly: bool,
}
impl Default for Preferences {
    fn default()->Self {Self {silent_start:true,pinch:true,follow_activity:true,fixed_family:"gpt".into(),muted:false,total:true,volume:0.5,reduce_motion:false,detailed:false,menu_scope:"pool".into(),menu_weekly:false}}
}
impl Preferences {
    pub fn validate(&self)->Result<(),String> {
        if !["pool","model","account"].contains(&self.menu_scope.as_str()) {return Err("Unknown menu-bar scope".into());}
        if !["gpt","claude","deepseek","gemini","grok","kimi","qwen","glm"].contains(&self.fixed_family.as_str()) {return Err("Unknown pet skin".into());}
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {return Err("Volume must be 0–1".into());}
        Ok(())
    }
}
#[tauri::command]
pub fn get_pet_preferences(state:tauri::State<crate::AppState>)->Result<Preferences,String> {
    Ok(state.store.lock().map_err(|e|e.to_string())?.settings.pet_preferences.clone())
}
#[tauri::command]
pub fn set_pet_preferences(app:tauri::AppHandle, preferences:Preferences)->Result<(),String> {
    preferences.validate()?;
    let state=app.state::<crate::AppState>();let mut store=state.store.lock().map_err(|e|e.to_string())?;
    let old=std::mem::replace(&mut store.settings.pet_preferences,preferences.clone());
    if let Err(e)=store.save(){store.settings.pet_preferences=old;return Err(e);}
    drop(store);let _=app.emit("pet-preferences",preferences);Ok(())
}
fn sound_path(which:&str)->Result<std::path::PathBuf,String> {
    if !["press","release"].contains(&which) {return Err("Unknown sound slot".into());}
    Ok(crate::app_paths::profile_home().ok_or("Missing profile")?.join("pet-sounds").join(format!("{which}.audio")))
}
fn mime(data:&[u8])->Result<&'static str,String> {
    if data.len()>2*1024*1024 {return Err("Sound must be at most 2 MiB".into());}
    if data.starts_with(b"ID3") || (data.len()>2 && data[0]==0xff && data[1]&0xe0==0xe0) {Ok("audio/mpeg")}
    else if data.starts_with(b"RIFF") && data.get(8..12)==Some(b"WAVE") {Ok("audio/wav")}
    else {Err("Choose an MP3 or WAV sound".into())}
}
#[tauri::command]
pub fn get_pet_sound(which:String)->Result<Option<String>,String> {
    let path=sound_path(&which)?;
    if !path.exists(){return Ok(None);}
    let meta=std::fs::symlink_metadata(&path).map_err(|e|e.to_string())?;
    if !meta.is_file() || meta.len()>2*1024*1024 {return Err("Invalid local sound".into());}
    let data=std::fs::read(path).map_err(|e|e.to_string())?;
    Ok(Some(format!("data:{};base64,{}",mime(&data)?,base64::engine::general_purpose::STANDARD.encode(data))))
}
#[tauri::command]
pub fn import_pet_sound(app:tauri::AppHandle, which:String, path:String)->Result<(),String> {
    let target=sound_path(&which)?;
    let meta=std::fs::metadata(&path).map_err(|e|e.to_string())?;
    if !meta.is_file() || meta.len()>2*1024*1024 {return Err("Sound must be a file at most 2 MiB".into());}
    let bytes=std::fs::read(path).map_err(|e|e.to_string())?;mime(&bytes)?;
    std::fs::create_dir_all(target.parent().unwrap()).map_err(|e|e.to_string())?;
    let temp=target.with_extension("tmp");std::fs::write(&temp,bytes).map_err(|e|e.to_string())?;
    std::fs::rename(temp,target).map_err(|e|e.to_string())?;
    let _=app.emit("pet-sounds-updated",());Ok(())
}
