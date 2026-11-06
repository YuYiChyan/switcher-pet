//! A separate local desktop surface; hiding it never stops the proxy.
use crate::AppState;
use serde::{Serialize, Deserialize};
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager, menu::{Menu,MenuItem,Submenu,CheckMenuItem,PredefinedMenuItem}};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Activity { pub model:String, pub source:String, pub account_id:String, pub at:i64, pub session_id:String, pub origin:String }
static ACTIVITY: OnceLock<Mutex<Activity>>=OnceLock::new();
fn activity() -> &'static Mutex<Activity> { ACTIVITY.get_or_init(Default::default) }

pub fn record(app: &tauri::AppHandle, model: &str, source: &str, account_id: &str, worker: bool) {
    if worker {return;}
    let value=Activity {model:model.into(),source:source.into(),account_id:account_id.into(),at:chrono::Utc::now().timestamp(),session_id:String::new(),origin:"proxy".into()};
    observe(app,value);
}

pub fn observe(app:&tauri::AppHandle, value:Activity) {
    if let Ok(mut slot)=activity().lock(){if value.at<slot.at || *slot==value{return;}*slot=value.clone();}
    let _=app.emit("pet-activity",value);
}
#[tauri::command]
pub fn get_pet_activity() -> Activity {activity().lock().map(|v|v.clone()).unwrap_or_default()}

#[tauri::command]
pub async fn open_pet(app: tauri::AppHandle) -> Result<(),String> {
    if let Some(win)=app.get_webview_window("pet") { return win.show().map_err(|e|e.to_string()); }
    let win=tauri::webview::WebviewWindowBuilder::new(&app,"pet",tauri::WebviewUrl::App("index.html".into()))
        .title("Switcher Pet · 桌宠").inner_size(320.0,280.0).decorations(false).transparent(true).shadow(false)
        .always_on_top(true).skip_taskbar(true).resizable(false).visible(false)
        .on_menu_event(|window,event| {
            let app=window.app_handle();let id=event.id().as_ref();
            match id {
                "pet-settings"=>{crate::tray::show_main_window_from_cmd(app);let _=app.emit("pet-open-settings",());},
                "pet-hide"=>{let _=window.hide();},
                "pet-mute"=>{let _=app.emit_to("pet","pet-toggle-mute",());},
                "pet-pinch" | "pet-follow"=>{
                    let state=app.state::<AppState>();
                    let mut prefs=state.store.lock().map(|s|s.settings.pet_preferences.clone()).unwrap_or_default();
                    if id=="pet-pinch"{prefs.pinch=!prefs.pinch;}else{prefs.follow_activity=!prefs.follow_activity;}
                    if let Err(e)=crate::pet_preferences::set_pet_preferences(app.clone(),prefs){let _=app.emit_to("pet","pet-error",e);}
                },
                "pet-pool"=>{let _=app.emit_to("pet","pet-toggle-pool",());},
                _=>{
                    if let Some(raw)=id.strip_prefix("pet-source:") {
                        if let Ok((account,model))=serde_json::from_str::<(String,String)>(raw) {
                            let state=app.state::<AppState>();
                            let result=(||{let mut store=state.store.lock().map_err(|e|e.to_string())?;
                                let before=store.settings.clone();
                                crate::model_routes::select_source(&mut store,&account,&model)?;
                                if let Err(e)=store.save(){store.settings=before;return Err(e);}Ok::<_,String>(())})();
                            if let Err(error)=result {let _=app.emit_to("pet","pet-error",error);}else{let _=app.emit("accounts-updated",());let _=app.emit_to("pet","pet-route-selected",model);}
                        }
                    }
                }
            }
        }).build().map_err(|e|e.to_string())?;
    position(&win)?;win.show().map_err(|e|e.to_string())
}
fn position(win: &tauri::WebviewWindow) -> Result<(),String> {
    let monitor=win.current_monitor().ok().flatten().or_else(||win.primary_monitor().ok().flatten()).ok_or("No display")?;
    let area=monitor.work_area();let scale=monitor.scale_factor();
    let size=win.outer_size().map_err(|e|e.to_string())?;
    let x=area.position.x+(area.size.width.saturating_sub(size.width)) as i32-(16.0*scale) as i32;
    let y=area.position.y+(area.size.height.saturating_sub(size.height)) as i32-(16.0*scale) as i32;
    win.set_position(tauri::PhysicalPosition::new(x.max(area.position.x),y.max(area.position.y))).map_err(|e|e.to_string())
}
#[tauri::command]
pub fn hide_pet(app:tauri::AppHandle) -> Result<(),String> {app.get_webview_window("pet").ok_or("Pet is not open")?.hide().map_err(|e|e.to_string())}
#[tauri::command]
pub fn drag_pet(window: tauri::WebviewWindow) -> Result<(),String> {
    if window.label()!="pet" {return Err("Only the pet window can be dragged".into());}
    window.start_dragging().map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn resize_pet(window:tauri::WebviewWindow, expanded:bool) -> Result<(),String> {
    if window.label()!="pet" {return Err("Only the pet can resize this way".into());}
    let old=window.outer_size().map_err(|e|e.to_string())?;
    let pos=window.outer_position().map_err(|e|e.to_string())?;
    let scale=window.scale_factor().map_err(|e|e.to_string())?;
    let height=if expanded{520.0}else{300.0};
    window.set_size(tauri::LogicalSize::new(320.0,height)).map_err(|e|e.to_string())?;
    let mut y=pos.y+old.height as i32-(height*scale) as i32;
    if let Some(monitor)=window.current_monitor().map_err(|e|e.to_string())? {y=y.max(monitor.work_area().position.y);}
    window.set_position(tauri::PhysicalPosition::new(pos.x,y)).map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn pet_menu(app:tauri::AppHandle) -> Result<(),String> {
    let state=app.state::<AppState>();
    let groups={let store=state.store.lock().map_err(|e|e.to_string())?;crate::model_routes::groups(&store)};
    let menu=Menu::new(&app).map_err(|e|e.to_string())?;
    for group in groups {
        if group["policy"]["visible"]==false {continue;}
        let model=group["model"].as_str().unwrap_or_default();
        let sub=Submenu::new(&app,model,true).map_err(|e|e.to_string())?;
        for source in group["sources"].as_array().into_iter().flatten() {
            let id=source["id"].as_str().unwrap_or_default();
            let item=CheckMenuItem::with_id(&app,format!("pet-source:{}",serde_json::to_string(&(id,model)).unwrap()),
                source["name"].as_str().unwrap_or("Account"),true,group["current"].as_str()==Some(id),None::<&str>).map_err(|e|e.to_string())?;
            sub.append(&item).map_err(|e|e.to_string())?;
        }
        menu.append(&sub).map_err(|e|e.to_string())?;
    }
    menu.append(&PredefinedMenuItem::separator(&app).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let prefs=state.store.lock().map(|s|s.settings.pet_preferences.clone()).unwrap_or_default();
    for (id,label,checked) in [("pet-pinch","捏一捏模式",prefs.pinch),("pet-follow","按模型切换形象",prefs.follow_activity)] {
        menu.append(&CheckMenuItem::with_id(&app,id,label,true,checked,None::<&str>).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    }
    for (id,label) in [("pet-pool","查看用量"),("pet-mute","切换静音"),("pet-settings","打开设置"),("pet-hide","隐藏桌宠（服务继续）")] {
        menu.append(&MenuItem::with_id(&app,id,label,true,None::<&str>).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    }
    app.get_webview_window("pet").ok_or("Pet is not open")?.popup_menu(&menu).map_err(|e|e.to_string())
}
