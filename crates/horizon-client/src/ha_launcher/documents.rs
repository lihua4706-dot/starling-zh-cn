//! SAF selection, bounded background imports and explicit collision choices.
use crate::ha_launcher::client::{
    Result,
    content_files::{self, Kind},
    hud::{HudAtlas, HudBatch, rgba},
    ui_theme::{Face, Ui, color},
};
use jni::{
    JNIEnv,
    objects::{JObject, JString},
};
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

struct Request {
    kind: Kind,
    appearance: Option<crate::ha_launcher::skins::library::Kind>,
    target_dir: Option<PathBuf>,
    home: PathBuf,
    cache: PathBuf,
}
struct Conflict {
    kind: Kind,
    staged: PathBuf,
    target: PathBuf,
}
#[derive(Clone)]
pub struct Notice {
    pub text: String,
    pub changed: bool,
    pub kind: Option<Kind>,
}
#[derive(Default)]
struct State {
    request: Option<Request>,
    working: bool,
    conflict: Option<Conflict>,
    notice: Option<Notice>,
    hits: Vec<([f32; 4], Choice)>,
    toast: Option<(String, std::time::Instant)>,
    cursor: (f32, f32),
}
#[derive(Clone, Copy)]
pub enum Choice {
    KeepBoth,
    Replace,
    Cancel,
}
static STATE: Mutex<State> = Mutex::new(State {
    request: None,
    working: false,
    conflict: None,
    notice: None,
    hits: Vec::new(),
    toast: None,
    cursor: (0.0, 0.0),
});
static CANCEL: AtomicBool = AtomicBool::new(false);
pub fn busy() -> bool {
    let s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    s.request.is_some() || s.working || s.conflict.is_some()
}
pub fn conflict() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .conflict
        .is_some()
}
fn notice(text: impl Into<String>, changed: bool) {
    let text = text.into();
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    s.toast = Some((text.clone(), std::time::Instant::now()));
    s.notice = Some(Notice {
        text,
        changed,
        kind: None,
    });
}
fn imported(text: impl Into<String>, kind: Kind) {
    let text = text.into();
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    s.toast = Some((text.clone(), std::time::Instant::now()));
    s.notice = Some(Notice {
        text,
        changed: true,
        kind: Some(kind),
    });
}
pub fn poll() -> Option<Notice> {
    if crate::ha_launcher::document_provider::take_changes() {
        return Some(Notice {
            text: "内容文件已更改，请重载资源包以应用。".into(),
            changed: true,
            kind: None,
        });
    }
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .notice
        .take()
}
pub fn begin(kind: Kind, home: PathBuf) -> std::result::Result<(), String> {
    if busy() {
        return Err("请先完成或取消当前导入".into());
    }
    CANCEL.store(false, Ordering::Relaxed);
    crate::ha_launcher::platform::hide();
    let cache = crate::ha_launcher::platform::cache_dir()
        .ok_or("无法访问安卓导入缓存")?
        .join("content-imports");
    STATE.lock().unwrap_or_else(|e| e.into_inner()).request = Some(Request {
        kind,
        appearance: None,
        target_dir: None,
        home,
        cache,
    });
    if !crate::ha_launcher::native_ime::pick_document() {
        picker_failed("安卓文档选择器不可用");
        return Err("安卓文档选择器不可用".into());
    }
    notice(format!("选择一个 {} 文件", kind.label()), false);
    Ok(())
}
pub fn picker_failed(error: &str) {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).request = None;
    notice(error, false);
}
pub fn begin_appearance(
    home: PathBuf,
    kind: crate::ha_launcher::skins::library::Kind,
) -> std::result::Result<(), String> {
    if busy() {
        return Err("请先完成或取消当前导入".into());
    }
    CANCEL.store(false, Ordering::Relaxed);
    crate::ha_launcher::platform::hide();
    let cache = crate::ha_launcher::platform::cache_dir()
        .ok_or("无法访问安卓导入缓存")?
        .join("appearance-imports");
    STATE.lock().unwrap_or_else(|e| e.into_inner()).request = Some(Request {
        kind: Kind::Packs,
        appearance: Some(kind),
        target_dir: None,
        home,
        cache,
    });
    if !crate::ha_launcher::native_ime::pick_document() {
        picker_failed("安卓文档选择器不可用");
        return Err("安卓文档选择器不可用".into());
    }
    notice("选择皮肤或披风 PNG", false);
    Ok(())
}

/// Opens Android's `ACTION_OPEN_DOCUMENT` picker to import a file directly into `target_dir`.
pub fn begin_into_dir(target_dir: PathBuf) -> std::result::Result<(), String> {
    if busy() {
        return Err("请先完成或取消当前导入".into());
    }
    CANCEL.store(false, Ordering::Relaxed);
    crate::ha_launcher::platform::hide();
    let cache = crate::ha_launcher::platform::cache_dir()
        .ok_or("无法访问安卓导入缓存")?
        .join("file-manager-imports");
    STATE.lock().unwrap_or_else(|e| e.into_inner()).request = Some(Request {
        kind: Kind::Packs,
        appearance: None,
        target_dir: Some(target_dir),
        home: PathBuf::new(),
        cache,
    });
    if !crate::ha_launcher::native_ime::pick_document() {
        picker_failed("安卓文档选择器不可用");
        return Err("安卓文档选择器不可用".into());
    }
    notice("选择要导入此文件夹的文件", false);
    Ok(())
}

/// Exports or shares a local file via Android MediaStore / SAF DocumentsUI.
pub fn export_file(
    home: &std::path::Path,
    profile: u32,
    file_path: &std::path::Path,
) -> std::result::Result<String, String> {
    let meta = std::fs::symlink_metadata(file_path).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("只能导出普通文件。".into());
    }
    let file_name = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("无效文件名")?;
    if file_name.to_ascii_lowercase().ends_with(".png") {
        let _ = crate::ha_launcher::gallery::save(file_path);
        return Ok(format!(
            "正在将 '{file_name}' 导出到 Pictures/HorizonAndroid…"
        ));
    }
    let rel_str = file_path
        .strip_prefix(home)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let saf_folder = if rel_str.contains("horizon-mods") {
        crate::ha_launcher::document_store::folder_id(profile, "mods")
    } else if rel_str.contains("resourcepacks") {
        crate::ha_launcher::document_store::folder_id(profile, "packs")
    } else if rel_str.contains("shaderpacks") {
        crate::ha_launcher::document_store::folder_id(profile, "shaders")
    } else if rel_str.contains("saves") {
        crate::ha_launcher::document_store::folder_id(profile, "worlds")
    } else if rel_str.contains("performance") {
        "reports".into()
    } else {
        crate::ha_launcher::document_store::ROOT.into()
    };
    if crate::ha_launcher::platform::open_documents(&saf_folder) {
        Ok(format!(
            "已在安卓文件中打开 '{file_name}' 以供共享/导出。"
        ))
    } else {
        Ok(format!(
            "已准备好通过系统文件 > Starling 导出 '{file_name}'。"
        ))
    }
}
/// Called by the approved Activity result bridge, never from a game tick:
/// `StarlingActivity.nativeDocumentResult` (android_lib.rs).
pub fn document_result(
    mut env: JNIEnv<'_>,
    activity: JObject<'_>,
    request: i32,
    result: i32,
    uri: JString<'_>,
) {
    if request != 4101 {
        return;
    }
    let pending = STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .request
        .take();
    let Some(pending) = pending else {
        notice("导入中断，请重新选择文件。", false);
        return;
    };
    if result != -1 || uri.is_null() {
        notice("已取消导入", false);
        return;
    }
    let setup = (|| -> jni::errors::Result<_> {
        let text: String = env.get_string(&uri)?.into();
        let activity = env.new_global_ref(activity)?;
        Ok((env.get_java_vm()?, activity, text))
    })();
    let Ok((vm, activity, uri)) = setup else {
        let _ = env.exception_clear();
        notice("无法读取所选文档", false);
        return;
    };
    if !uri.starts_with("content://") || uri.len() > 8192 {
        notice("不支持的文档 URI", false);
        return;
    }
    STATE.lock().unwrap_or_else(|e| e.into_inner()).working = true;
    let spawned = std::thread::Builder::new()
        .name("内容导入".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let mut env = vm.attach_current_thread()?;
                let fallback = if pending.kind == Kind::Mods {
                    "import.hmod"
                } else {
                    "import.zip"
                };
                let opened = crate::ha_launcher::document_bridge::open(
                    &mut env,
                    activity.as_obj(),
                    &uri,
                    fallback,
                );
                if opened.is_err() {
                    let _ = env.exception_clear();
                }
                let (input, name) = opened.map_err(|e| e.to_string())?;
                stage(pending, input, name)
            })();
            STATE.lock().unwrap_or_else(|e| e.into_inner()).working = false;
            if let Err(error) = result {
                notice(format!("导入失败：{error}"), false);
            }
        });
    if let Err(error) = spawned {
        STATE.lock().unwrap_or_else(|e| e.into_inner()).working = false;
        notice(format!("导入失败：{error}"), false);
    }
}
fn stage(request: Request, mut input: std::fs::File, name: String) -> Result<()> {
    if let Some(target_dir) = request.target_dir {
        crate::ha_launcher::document_store::validate_entry_name(&name)?;
        std::fs::create_dir_all(&target_dir)?;
        if std::fs::symlink_metadata(&target_dir)?
            .file_type()
            .is_symlink()
        {
            return Err("拒绝重定向的目标文件夹".into());
        }
        std::fs::create_dir_all(&request.cache)?;
        if std::fs::symlink_metadata(&request.cache)?
            .file_type()
            .is_symlink()
        {
            return Err("拒绝重定向的导入缓存".into());
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let staged = request.cache.join(format!(".import-fm-{stamp}.part"));
        let res = (|| -> Result<()> {
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)?;
            content_files::copy_bounded(&mut input, &mut output, |bytes| {
                notice(format!("正在导入 {name}：{} KiB", bytes / 1024), false);
                !CANCEL.load(Ordering::Relaxed)
            })?;
            output.sync_all()?;
            drop(output);
            if CANCEL.load(Ordering::Relaxed) {
                return Err("已取消导入".into());
            }
            let dest =
                crate::ha_launcher::document_store::unique_destination_path(&target_dir, &name)?;
            let final_name = dest
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if std::fs::rename(&staged, &dest).is_err() {
                std::fs::copy(&staged, &dest)?;
                let _ = std::fs::remove_file(&staged);
            }
            notice(
                format!("已将 '{final_name}' 导入当前文件夹。"),
                true,
            );
            Ok(())
        })();
        if res.is_err() {
            let _ = std::fs::remove_file(&staged);
        }
        return res;
    }
    if let Some(kind) = request.appearance {
        use std::io::Read;
        if !name.to_ascii_lowercase().ends_with(".png") {
            return Err("选择 PNG 皮肤或披风".into());
        }
        let mut bytes = Vec::new();
        input
            .take(crate::ha_launcher::skins::MAX_IMPORT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if CANCEL.load(Ordering::Relaxed) {
            return Err("已取消导入".into());
        }
        let mut library = crate::ha_launcher::skins::library::Library::load(&request.home)?;
        library.import(&request.home, &name, kind, &bytes)?;
        notice(
            "PNG 已加入本地图库，选择它以在本地应用。",
            false,
        );
        return Ok(());
    }
    if !content_files::plain_name(&name) || !request.kind.accepts(&name) {
        return Err("所选文件扩展名与此文件夹不匹配".into());
    }
    let dir = request.home.join(request.kind.folder());
    std::fs::create_dir_all(&dir)?;
    if std::fs::symlink_metadata(&dir)?.file_type().is_symlink() {
        return Err("拒绝重定向的内容目录".into());
    }
    std::fs::create_dir_all(&request.cache)?;
    if std::fs::symlink_metadata(&request.cache)?
        .file_type()
        .is_symlink()
    {
        return Err("拒绝重定向的导入缓存".into());
    }
    for entry in std::fs::read_dir(&request.cache)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".import-") && name.ends_with(".part") && entry.file_type()?.is_file() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let staged = request.cache.join(format!(".import-{stamp}.part"));
    let result = (|| -> Result<()> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
        content_files::copy_bounded(&mut input, &mut output, |bytes| {
            notice(
                format!("正在导入 {}：{} KiB", request.kind.label(), bytes / 1024),
                false,
            );
            !CANCEL.load(Ordering::Relaxed)
        })?;
        output.sync_all()?;
        drop(output);
        notice("正在检查压缩包…", false);
        let native_shader = content_files::validate(&staged, request.kind, &name, || {
            CANCEL.load(Ordering::Relaxed)
        })?;
        let name = if request.kind == Kind::Mods {
            format!(
                "{}.hmod",
                horizon_mod_package::Package::read(&staged)?.manifest.id
            )
        } else if native_shader || name.to_ascii_lowercase().ends_with(".hshader") {
            let pack = horizon_shader_api::Pack::read(&staged)?;
            horizon_shader_api::check(&pack).map_err(|e| format!("无效的光影包：{e:?}"))?;
            let mut path = PathBuf::from(name);
            path.set_extension("hshader");
            path.to_string_lossy().into_owned()
        } else {
            name
        };
        if !content_files::plain_name(&name) {
            return Err("包 ID 或文件名无效".into());
        }
        if CANCEL.load(Ordering::Relaxed) {
            return Err("已取消导入".into());
        }
        let target = dir.join(name);
        if target.exists() {
            STATE.lock().unwrap_or_else(|e| e.into_inner()).conflict = Some(Conflict {
                kind: request.kind,
                staged: staged.clone(),
                target,
            });
            notice(
                "文件已存在：选择保留两者、替换或取消",
                false,
            );
        } else {
            std::fs::rename(&staged, &target)?;
            imported(
                format!(
                    "已将 {} 导入 {}。在游戏设置中启用。",
                    target.file_name().unwrap_or_default().to_string_lossy(),
                    request.kind.label()
                ),
                request.kind,
            );
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(staged);
    }
    result
}
pub fn resolve(choice: Choice) {
    let c = STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .conflict
        .take();
    let Some(mut c) = c else {
        return;
    };
    if matches!(choice, Choice::Cancel) {
        let _ = std::fs::remove_file(c.staged);
        notice("已取消导入", false);
        return;
    }
    let result = (|| -> Result<()> {
        if matches!(choice, Choice::Replace)
            && std::fs::symlink_metadata(&c.target)
                .is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
        {
            return Err("无法替换文件夹或符号链接".into());
        }
        if matches!(choice, Choice::KeepBoth) {
            if c.kind == Kind::Mods {
                return Err(
                    "模组 ID 不能安装两次；选择「替换」来更新它".into(),
                );
            }
            let mut stem = c
                .target
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            while stem.len() > 150 {
                stem.pop();
            }
            let ext = c
                .target
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let dir = c
                .target
                .parent()
                .ok_or("无效的内容文件夹")?
                .to_owned();
            for i in 1..=10000 {
                let candidate = dir.join(format!("{stem} ({i}).{ext}"));
                if !candidate.exists() {
                    c.target = candidate;
                    break;
                }
                if i == 10000 {
                    return Err("文件名冲突过多".into());
                }
            }
        }
        std::fs::rename(&c.staged, &c.target)?;
        Ok(())
    })();
    match result {
        Ok(()) => imported(
            "导入完成，请在“已安装”或游戏设置中启用。",
            c.kind,
        ),
        Err(e) => {
            let _ = std::fs::remove_file(c.staged);
            notice(format!("导入失败：{e}"), false);
        }
    }
}
pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
    STATE.lock().unwrap_or_else(|e| e.into_inner()).request = None;
    if conflict() {
        resolve(Choice::Cancel);
    } else {
        notice("已取消导入", false);
    }
}
pub fn tap(x: f32, y: f32) -> bool {
    if !busy() {
        return false;
    }
    let choice = STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .hits
        .iter()
        .find(|(r, _)| x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3])
        .map(|(_, c)| *c);
    if let Some(choice) = choice {
        if matches!(choice, Choice::Cancel) {
            cancel();
        } else {
            resolve(choice);
        }
    }
    true
}
pub fn draw(batch: &mut HudBatch, atlas: &HudAtlas) {
    let mut ui = Ui::mobile(atlas, batch, (0.0, 0.0));
    draw_ui(&mut ui);
    ui.finish();
}
/// Launcher overlays must use its bundled font, including before game fonts exist.
pub fn draw_ui(ui: &mut Ui<'_>) {
    let mut s = STATE.lock().unwrap_or_else(|e| e.into_inner());
    s.hits.clear();
    if s.conflict.is_none() && !s.working {
        // The launcher already shows the result in its status line. Keep only
        // actionable overlays (progress/cancel and a name-conflict choice).
        return;
    }
    let (name, choices) = if let Some(c) = &s.conflict {
        (
            c.target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            if c.kind == Kind::Mods {
                vec![("替换", Choice::Replace), ("取消", Choice::Cancel)]
            } else {
                vec![
                    ("保留两者", Choice::KeepBoth),
                    ("替换", Choice::Replace),
                    ("取消", Choice::Cancel),
                ]
            },
        )
    } else {
        (
            s.toast.as_ref().map(|v| v.0.clone()).unwrap_or_default(),
            vec![("取消", Choice::Cancel)],
        )
    };
    let w = 440.0f32.min(ui.w - 24.0);
    let vertical = w < 320.0;
    let height = if vertical {
        92.0 + choices.len() as f32 * 44.0
    } else {
        144.0
    };
    let x = (ui.w - w) / 2.0;
    let y = ((ui.h - height) / 2.0).max(0.0);
    ui.rounded(0.0, 0.0, ui.w, ui.h, 0.0, rgba(0, 0, 0, 180));
    ui.rounded(x, y, w, height, 2.0, color::BG);
    ui.text(
        Face::Medium,
        20.0,
        x + 12.0,
        y + 12.0,
        color::TEXT,
        if s.conflict.is_some() {
            "文件已存在"
        } else {
            "正在导入文件"
        },
    );
    let name = ui.fit_text(Face::Regular, 16.0, &name, w - 24.0);
    ui.text(Face::Regular, 16.0, x + 12.0, y + 46.0, color::MUTED, &name);
    let width = (w - 24.0) / choices.len() as f32;
    for (i, (label, choice)) in choices.into_iter().enumerate() {
        let rect = if vertical {
            [x + 12.0, y + 86.0 + i as f32 * 44.0, w - 24.0, 38.0]
        } else {
            [x + 12.0 + i as f32 * width, y + 92.0, width - 4.0, 38.0]
        };
        ui.rounded(rect[0], rect[1], rect[2], rect[3], 2.0, color::RAISED);
        ui.text(
            Face::Medium,
            16.0,
            rect[0] + 8.0,
            rect[1] + 7.0,
            color::TEXT,
            label,
        );
        s.hits.push((
            [
                rect[0] * ui.density,
                rect[1] * ui.density,
                rect[2] * ui.density,
                rect[3] * ui.density,
            ],
            choice,
        ));
    }
}
pub fn handle_event(event: &winit::event::WindowEvent) -> bool {
    use winit::{
        event::{ElementState, MouseButton, TouchPhase, WindowEvent},
        keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    };
    if !busy() {
        return false;
    }
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            STATE.lock().unwrap_or_else(|e| e.into_inner()).cursor =
                (position.x as f32, position.y as f32);
            true
        }
        WindowEvent::MouseInput {
            button: MouseButton::Left,
            state,
            ..
        } => {
            if *state == ElementState::Released {
                let (x, y) = STATE.lock().unwrap_or_else(|e| e.into_inner()).cursor;
                tap(x, y);
            }
            true
        }
        WindowEvent::Touch(t) => {
            if t.phase == TouchPhase::Ended {
                tap(t.location.x as f32, t.location.y as f32);
            }
            true
        }
        WindowEvent::KeyboardInput { event, .. } => {
            if event.state == ElementState::Pressed {
                if event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                    || event.logical_key == Key::Named(NamedKey::BrowserBack)
                {
                    cancel();
                } else if event.physical_key == PhysicalKey::Code(KeyCode::Enter) && conflict() {
                    let native = STATE
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .conflict
                        .as_ref()
                        .is_some_and(|c| c.kind == Kind::Mods);
                    resolve(if native {
                        Choice::Cancel
                    } else {
                        Choice::KeepBoth
                    });
                }
            }
            true
        }
        _ => false,
    }
}
