//! Thin native view over the shared account and wardrobe APIs.
use super::{
    login::Login,
    pages::{Action, button},
};
use crate::ha_launcher::client::{
    hud::rgba,
    skin_head::{self, HeadSkin},
    ui_theme::{Face, Ui},
};
use crate::ha_launcher::{
    account::{Accounts, Provider},
    skins::{
        self, Model, cloud,
        library::{Kind, Library},
        preview,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

pub(super) enum CloudAction {
    Upload,
    Cape(usize),
}
pub(super) struct Wardrobe {
    pub root: PathBuf,
    default_skin: PathBuf,
    default_worker: Option<mpsc::Receiver<Result<Vec<u8>, String>>>,
    library: Library,
    key: String,
    provider: Option<Provider>,
    pub capes: bool,
    pub camera: preview::Camera,
    cache: cloud::Cache,
    pub view: [f32; 3],
    worker: Option<mpsc::Receiver<(String, Result<(cloud::Cache, bool), String>)>>,
    bitmap: Vec<([f32; 4], u32)>,
    rect: [f32; 4],
    fingers: std::collections::BTreeMap<u64, [f32; 2]>,
    note: String,
    damaged: bool,
    /// The account whose appearance is shown, used to key the head cache.
    name: String,
    /// Decoded head of the selected appearance, drawn by the launcher header.
    head: Option<HeadSkin>,
    pub tab_changed_at: Option<std::time::Instant>,
    pub selection_changed_at: Option<std::time::Instant>,
}
impl Wardrobe {
    pub fn new(home: &Path) -> Self {
        let root = home.join("wardrobe");
        let loaded = Library::load(&root);
        let damaged = loaded.is_err();
        let note = loaded.as_ref().err().cloned().unwrap_or_default();
        let mut value = Self {
            root,
            // Starling's prepared resources (the app's own files).
            default_skin: crate::ha_launcher::client::logging::private_home()
                .join("resources/assets/minecraft/textures/entity/player/wide/steve.png"),
            default_worker: None,
            library: loaded.unwrap_or_default(),
            key: String::new(),
            provider: None,
            capes: false,
            camera: Default::default(),
            cache: Default::default(),
            worker: None,
            bitmap: vec![],
            rect: [0.0; 4],
            view: [0.0; 3],
            fingers: Default::default(),
            note,
            damaged,
            name: String::new(),
            head: None,
            tab_changed_at: None,
            selection_changed_at: None,
        };
        value.ensure_default_skin();
        value.preview();
        value
    }
    // Fresh/older installations may not have vanilla player textures yet. Fetch
    // the official template once in the background; never ship Mojang's pixels.
    fn ensure_default_skin(&mut self) {
        if skins::read_file(&self.default_skin)
            .ok()
            .and_then(|b| skins::read_skin(&b).ok())
            .is_some()
        {
            return;
        }
        let cached = self.root.join("default-steve.png");
        if skins::read_file(&cached)
            .ok()
            .and_then(|b| skins::read_skin(&b).ok())
            .is_some()
        {
            self.default_skin = cached;
            return;
        }
        if self.default_worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.default_worker = Some(rx);
        std::thread::spawn(move || {
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let result = (|| {
                let url = "https://assets.mojang.com/SkinTemplates/steve.png";
                let agent = crate::ha_launcher::proxy::agent(
                    ureq::AgentBuilder::new()
                        .timeout_connect(std::time::Duration::from_secs(10))
                        .timeout_read(std::time::Duration::from_secs(15)),
                    url,
                )
                .build();
                let mut bytes = Vec::new();
                agent
                    .get(url)
                    .call()
                    .map_err(|e| e.to_string())?
                    .into_reader()
                    .take(skins::MAX_IMPORT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                skins::read_skin(&bytes)?;
                if format!("{:x}", Sha256::digest(&bytes))
                    != "4c7b0468044bfecacc43d00a3a69335a834b73937688292c20d3988cae58248d"
                {
                    return Err("官方默认皮肤校验和不匹配".into());
                }
                Ok(bytes)
            })();
            let _ = tx.send(result);
        });
    }
    pub fn open(&mut self, offline: &str, client_id: &str) {
        self.ensure_default_skin();
        let account = Accounts::load().selected().cloned();
        let name = account
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| offline.to_owned());
        let key = account
            .as_ref()
            .map(|a| a.key())
            .unwrap_or_else(|| format!("offline:{}", offline.to_ascii_lowercase()));
        if self.key != key {
            // A previous provider's in-flight result must not delay this profile's cloud load.
            self.worker = None;
            self.fingers.clear();
            self.key = key.clone();
            self.provider = account.map(|a| a.provider);
            self.cache = Default::default();
            // The last head is on disk: show it on the first frame, then let
            // the local selection or cloud sync replace it.
            self.head = self.read_head(&key);
        }
        self.name = name;
        if let Some(head) = self.head {
            skin_head::remember(&self.name, head);
        }
        self.reload();
        if self.provider.is_some() && self.worker.is_none() {
            self.cloud(client_id, None);
        }
        self.preview();
    }
    pub fn reload(&mut self) {
        match Library::load(&self.root) {
            Ok(value) => {
                self.library = value;
                self.damaged = false;
                self.preview();
            }
            Err(error) => {
                self.damaged = true;
                self.note = error;
            }
        }
    }
    pub fn poll(&mut self) {
        if let Some(result) = self
            .default_worker
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.default_worker = None;
            match result {
                Ok(bytes) => {
                    let path = self.root.join("default-steve.png");
                    if std::fs::create_dir_all(&self.root)
                        .and_then(|_| std::fs::write(&path, bytes))
                        .is_ok()
                    {
                        self.default_skin = path;
                        self.preview();
                    }
                }
                Err(error) => log::warn!("默认皮肤不可用：{error}"),
            }
        }
        if let Some((key, result)) = self.worker.as_ref().and_then(|r| r.try_recv().ok()) {
            self.worker = None;
            if key != self.key {
                return;
            }
            match result {
                Ok((cache, activated_cape)) => {
                    let selection = self.library.selections.entry(self.key.clone()).or_default();
                    if selection.skin.is_none() {
                        selection.model = if cache.slim {
                            Model::Slim
                        } else {
                            Model::Classic
                        };
                    }
                    if activated_cape {
                        selection.cape = None;
                        selection.hide_cape = false;
                        if let Err(error) = self.library.save(&self.root) {
                            self.note = error;
                            self.preview();
                            return;
                        }
                    }
                    self.cache = cache;
                    self.note =
                        "云端档案已同步，本地覆盖保持独立。".into();
                    self.preview();
                }
                Err(error) => self.note = error,
            }
        }
    }
    fn kind(&self) -> Kind {
        if self.capes { Kind::Cape } else { Kind::Skin }
    }
    fn save(&mut self) {
        if let Err(error) = self.library.save(&self.root) {
            self.note = error;
        }
        self.preview();
    }
    pub fn select(&mut self, index: usize) {
        if self.damaged || crate::ha_launcher::documents::busy() {
            return;
        }
        if let Some(entry) = self.library.entries.get(index).cloned() {
            self.note = match self
                .library
                .select(&self.root, &self.key, &entry.id, entry.kind)
            {
                Ok(()) => "已在本地应用，账号的云端档案未改变。".into(),
                Err(error) => error,
            };
            self.selection_changed_at = Some(std::time::Instant::now());
            self.preview();
        }
    }
    pub fn import(&mut self) {
        if self.damaged {
            return;
        }
        self.note =
            match crate::ha_launcher::documents::begin_appearance(self.root.clone(), self.kind()) {
                Ok(()) => "在安卓文件中选择一张 PNG 图片。".into(),
                Err(error) => error,
            };
    }
    pub fn use_cloud(&mut self) {
        if self.damaged || crate::ha_launcher::documents::busy() {
            return;
        }
        let selection = self.library.selections.entry(self.key.clone()).or_default();
        if self.capes {
            selection.cape = None;
            selection.hide_cape = false;
        } else {
            selection.skin = None;
        }
        self.note = if self.provider.is_some() {
            "正在使用账号的云端外观，未做云端更改。"
        } else {
            "已清除本地覆盖，使用默认外观。"
        }
        .into();
        self.save();
    }
    pub fn model(&mut self) {
        if self.damaged || crate::ha_launcher::documents::busy() {
            return;
        }
        let selection = self.library.selections.entry(self.key.clone()).or_default();
        selection.model = if selection.model == Model::Classic {
            Model::Slim
        } else {
            Model::Classic
        };
        self.save();
    }
    pub fn hide_cape(&mut self) {
        if self.damaged || crate::ha_launcher::documents::busy() {
            return;
        }
        let selection = self.library.selections.entry(self.key.clone()).or_default();
        selection.hide_cape = !selection.hide_cape;
        self.save();
    }
    pub fn cloud(&mut self, client_id: &str, action: Option<CloudAction>) {
        if self.worker.is_some() {
            return;
        }
        let Some(provider) = self.provider else {
            self.note = "离线外观仅对本安装生效。".into();
            return;
        };
        if provider == Provider::Ely && action.is_some() {
            self.note =
                "Ely.by 云端皮肤在 ely.by 管理，自定义披风可在本地应用。"
                    .into();
            if let Err(error) = crate::ha_launcher::platform::open_login_url("https://ely.by/skins")
            {
                self.note = error;
            }
            return;
        }
        let key = self.key.clone();
        let root = self.root.clone();
        let id = client_id.to_owned();
        let selection = self
            .library
            .selections
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let cape = match &action {
            Some(CloudAction::Cape(index)) => {
                self.cache.snapshot.capes.get(*index).map(|c| c.id.clone())
            }
            _ => None,
        };
        if matches!(action, Some(CloudAction::Upload)) && selection.skin.is_none() {
            self.note = "请先选择一个本地皮肤再上传到微软账号。".into();
            return;
        }
        if matches!(action, Some(CloudAction::Cape(_))) && cape.is_none() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.worker = Some(receiver);
        self.note = "正在连接账号服务…".into();
        std::thread::spawn(move || {
            let result = (|| {
                let (account, secrets) = Login::session(&id)?
                    .ok_or("请先登录再同步云端外观。")?;
                if account.key() != key {
                    return Err("所选账号已更改，请重新打开图库。".into());
                }
                match action {
                    Some(CloudAction::Upload) => {
                        let path = Library::image(
                            &root,
                            selection.skin.as_deref().ok_or("请先选择一个皮肤。")?,
                        )?;
                        cloud::upload_skin(
                            &account,
                            &secrets,
                            &skins::read_file(&path)?,
                            selection.model,
                        )?;
                    }
                    Some(CloudAction::Cape(_)) => {
                        cloud::activate_cape(&account, &secrets, cape.as_deref())?
                    }
                    None => {}
                }
                cloud::fetch(&account, &secrets, &root)
                    .map(|cache| (cache, matches!(action, Some(CloudAction::Cape(_)))))
            })();
            let _ = sender.send((key, result));
        });
    }
    pub fn prepare_game(&mut self, offline: &str, game: &Path) -> Result<(), String> {
        let key = Accounts::load()
            .selected()
            .map(|a| a.key())
            .unwrap_or_else(|| format!("offline:{}", offline.to_ascii_lowercase()));
        let cloud = if key == self.key {
            self.cache
                .skin
                .as_deref()
                .map(|p| (p, self.cache.slim, self.cache.cape.as_deref()))
        } else {
            None
        };
        Library::load(&self.root)?.prepare_game(&self.root, &key, game, cloud)
    }
    fn head_path(&self, key: &str) -> PathBuf {
        let safe: String = key
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .take(96)
            .collect();
        self.root.join("heads").join(format!("{safe}.json"))
    }

    fn read_head(&self, key: &str) -> Option<HeadSkin> {
        let bytes = std::fs::read(self.head_path(key)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Keep the header head in memory and on disk, so the next launch paints
    /// the real face on its first frame without opening Skins first.
    fn set_head(&mut self, head: Option<HeadSkin>) {
        if self.head == head {
            return;
        }
        self.head = head;
        let Some(head) = head else {
            return;
        };
        if !self.name.is_empty() {
            skin_head::remember(&self.name, head);
        }
        if self.key.is_empty() {
            return;
        }
        let path = self.head_path(&self.key);
        if let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            return;
        }
        if let Ok(bytes) = serde_json::to_vec(&head) {
            let _ = std::fs::write(path, bytes);
        }
    }

    pub fn preview(&mut self) {
        let selection = self
            .library
            .selections
            .get(&self.key)
            .cloned()
            .unwrap_or_default();
        let root = self.root.clone();
        let path =
            |id: &Option<String>| id.as_deref().and_then(|id| Library::image(&root, id).ok());
        let chosen = path(&selection.skin)
            .or_else(|| self.cache.skin.clone())
            .and_then(|p| skins::read_file(&p).ok())
            .and_then(|b| skins::read_skin(&b).ok());
        let skin = chosen.clone().or_else(|| {
            let bytes = skins::read_file(&self.default_skin).ok()?;
            skins::read_skin(&bytes).ok()
        });
        let Some(skin) = skin else {
            // Do not present a gray checkerboard as the player's default skin.
            self.bitmap.clear();
            return;
        };
        // A real selection owns the head; with none, keep the cached cloud
        // head until the cloud fetch answers, rather than flashing Steve.
        if let Some(image) = &chosen {
            self.set_head(skin_head::from_rgba(
                image.width(),
                image.height(),
                image.as_raw(),
            ));
        } else if self.head.is_none() {
            self.set_head(skin_head::from_rgba(
                skin.width(),
                skin.height(),
                skin.as_raw(),
            ));
        }
        let cape = if selection.hide_cape {
            None
        } else {
            path(&selection.cape)
                .or_else(|| self.cache.cape.clone())
                .and_then(|p| skins::read_file(&p).ok())
                .and_then(|b| skins::read_cape(&b).ok())
        };
        let model = selection.model;
        let image = preview::render(&skin, cape.as_ref(), model, self.camera, [160, 200]);
        self.bitmap.clear();
        for y in 0..image.height() {
            let mut x = 0;
            while x < image.width() {
                let color = image.get_pixel(x, y).0;
                let start = x;
                x += 1;
                while x < image.width() && image.get_pixel(x, y).0 == color {
                    x += 1;
                }
                if color[3] > 0 {
                    self.bitmap.push((
                        [start as f32, y as f32, (x - start) as f32, 1.0],
                        rgba(color[0], color[1], color[2], color[3]),
                    ));
                }
            }
        }
    }
    pub fn is_animating(&self) -> bool {
        let now = std::time::Instant::now();
        self.worker.is_some()
            || crate::ha_launcher::documents::busy()
            || self
                .tab_changed_at
                .is_some_and(|t| now.duration_since(t).as_secs_f32() < 0.22)
            || self
                .selection_changed_at
                .is_some_and(|t| now.duration_since(t).as_secs_f32() < 0.22)
    }

    pub fn set_tab(&mut self, capes: bool) {
        if self.capes != capes {
            self.capes = capes;
            self.tab_changed_at = Some(std::time::Instant::now());
            // Orient camera to showcase the back when switching to Capes, or front when switching to Skins
            if capes && (self.camera.yaw < 70.0 || self.camera.yaw > 290.0) {
                self.camera.yaw = 155.0;
                self.camera.pitch = -6.0;
            } else if !capes && (self.camera.yaw >= 90.0 && self.camera.yaw <= 270.0) {
                self.camera.yaw = 335.0;
                self.camera.pitch = -8.0;
            }
            self.preview();
        }
    }

    pub fn gesture(&mut self, touch: &winit::event::Touch, density: f32) -> bool {
        use winit::event::TouchPhase;
        let point = [
            touch.location.x as f32 / density,
            touch.location.y as f32 / density,
        ];
        if touch.phase == TouchPhase::Started {
            let [x, y, w, h] = self.rect;
            if point[0] >= x && point[0] < x + w && point[1] >= y && point[1] < y + h {
                self.fingers.insert(touch.id, point);
                return true;
            }
            return false;
        }
        let Some(old) = self.fingers.get(&touch.id).copied() else {
            return false;
        };
        if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.fingers.remove(&touch.id);
            return true;
        }
        if let Some(other) = self
            .fingers
            .iter()
            .find_map(|(id, p)| (*id != touch.id).then_some(*p))
        {
            let distance =
                |p: [f32; 2]| ((p[0] - other[0]).powi(2) + (p[1] - other[1]).powi(2)).sqrt();
            let before = distance(old);
            if before > 1.0 {
                self.camera.zoom(distance(point) / before);
            }
        } else {
            // Horizontal & vertical drag matches user finger direction in both front and back views
            self.camera.drag(point[0] - old[0], point[1] - old[1]);
        }
        self.fingers.insert(touch.id, point);
        self.preview();
        true
    }

    pub fn draw(
        &mut self,
        ui: &mut Ui,
        targets: &mut Vec<([f32; 4], Action)>,
        scroll: f32,
        x: f32,
        w: f32,
        top: f32,
    ) {
        use crate::ha_launcher::client::ui_theme::{Icon, active_theme, motion};
        let theme = active_theme();
        let now = std::time::Instant::now();
        let tab_t = motion::ease_out_cubic(motion::progress(self.tab_changed_at, now, 0.20));
        let sel_t = motion::spring_spatial(motion::progress(self.selection_changed_at, now, 0.22));
        let slide_y = (1.0 - tab_t) * 8.0;

        let h = ui.h;
        let wide = w >= 580.0;

        // 1. Header & Segmented Skins / Capes Switcher
        ui.text(
            Face::SemiBold,
            19.0,
            x,
            top,
            theme.text_primary,
            "皮肤与披风工坊",
        );
        let subtitle = ui.fit_text(
            Face::Regular,
            11.5,
            if self.capes {
                "预览 3D 披风、管理本地披风覆盖，或启用云端账号披风。"
            } else {
                "查看你的 3D 角色模型、切换手臂样式，并管理本地或云端皮肤。"
            },
            w,
        );
        ui.text(
            Face::Regular,
            11.5,
            x,
            top + 24.0,
            theme.text_muted,
            &subtitle,
        );

        let skin_count = self
            .library
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Skin)
            .count();
        let cape_count = self
            .library
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Cape)
            .count()
            + if self.provider.is_some() {
                self.cache.snapshot.capes.len()
            } else {
                0
            };

        let tabs_y = top + 46.0;
        ui.rounded(x, tabs_y, w, 38.0, 12.0, theme.surface_dim);
        ui.border(x, tabs_y, w, 38.0, 12.0, theme.border);
        let tw = (w - 8.0) * 0.5;
        for (i, (lbl, is_cape, active)) in [
            (format!("皮肤 ({skin_count})"), false, !self.capes),
            (format!("披风 ({cape_count})"), true, self.capes),
        ]
        .into_iter()
        .enumerate()
        {
            let tx = x + 3.0 + i as f32 * (tw + 2.0);
            let tr = [tx, tabs_y + 3.0, tw, 32.0];
            if active {
                ui.rounded(tr[0], tr[1], tr[2], tr[3], 9.0, theme.accent);
                ui.centered_fit(Face::SemiBold, 12.0, tr, theme.on_accent, &lbl);
            } else {
                ui.centered_fit(Face::Medium, 12.0, tr, theme.text_muted, &lbl);
            }
            targets.push((tr, Action::WardrobeKind(is_cape)));
        }

        let selection = self
            .library
            .selections
            .get(&self.key)
            .cloned()
            .unwrap_or_default();
        let selected_id = if self.capes {
            selection.cape.as_deref()
        } else {
            selection.skin.as_deref()
        };
        let model = selection.model;

        // 2. Left / Top Studio 3D Preview Card
        let preview_y = tabs_y + 48.0;
        let preview_w = if wide { 248.0_f32.min(w * 0.41) } else { w };
        let preview_h = if wide {
            (h - preview_y - 84.0).clamp(165.0, 276.0)
        } else {
            (h * 0.30).clamp(138.0, 192.0)
        };
        let px = x;
        self.rect = [px, preview_y, preview_w, preview_h];

        // Studio stage background with subtle inner rim and pedestal
        ui.rounded(px, preview_y, preview_w, preview_h, 16.0, theme.surface);
        ui.rounded(
            px + 4.0,
            preview_y + 4.0,
            preview_w - 8.0,
            preview_h - 8.0,
            13.0,
            theme.surface_dim,
        );
        ui.stroke_rounded(px, preview_y, preview_w, preview_h, 16.0, 1.2, theme.border);

        // Studio floor pedestal oval beneath the character
        let ped_w = (preview_w * 0.46).clamp(72.0, 124.0);
        let ped_h = 12.0;
        let ped_x = px + (preview_w - ped_w) * 0.5;
        let ped_y = preview_y + preview_h - 48.0;
        ui.rounded(ped_x, ped_y, ped_w, ped_h, 6.0, theme.accent_soft);
        ui.stroke_rounded(ped_x, ped_y, ped_w, ped_h, 6.0, 1.0, theme.border);

        // Top-left status badge inside studio card
        let badge_txt = if self.capes {
            if selection.hide_cape {
                "隐藏披风"
            } else {
                "3D 披风视图"
            }
        } else if model == Model::Slim {
            "纤细（3px 手臂）"
        } else {
            "经典（4px 手臂）"
        };
        let badge_w = (ui.measure(Face::SemiBold, 9.5, badge_txt) + 16.0).min(preview_w * 0.55);
        ui.rounded(
            px + 10.0,
            preview_y + 10.0,
            badge_w,
            20.0,
            6.0,
            theme.surface_selected,
        );
        ui.border(
            px + 10.0,
            preview_y + 10.0,
            badge_w,
            20.0,
            6.0,
            theme.accent,
        );
        ui.centered_fit(
            Face::SemiBold,
            9.5,
            [px + 10.0, preview_y + 10.0, badge_w, 20.0],
            theme.text_primary,
            badge_txt,
        );

        // Top-right orbit & zoom hint
        let hint = format!("{:.0}%", self.camera.zoom * 100.0);
        let hint_w = 42.0;
        ui.rounded(
            px + preview_w - hint_w - 10.0,
            preview_y + 10.0,
            hint_w,
            20.0,
            6.0,
            theme.surface,
        );
        ui.centered_fit(
            Face::Mono,
            9.0,
            [
                px + preview_w - hint_w - 10.0,
                preview_y + 10.0,
                hint_w,
                20.0,
            ],
            theme.text_muted,
            &hint,
        );

        // Render 3D character bitmap with subtle selection pop scale
        let pop = 0.97 + 0.03 * sel_t;
        let scale = ((preview_w / 160.0).min((preview_h - 18.0) / 200.0)) * pop;
        let ox = px + (preview_w - 160.0 * scale) * 0.5;
        let oy = preview_y + (preview_h - 14.0 - 200.0 * scale) * 0.5;
        if self.bitmap.is_empty() {
            ui.centered_fit(
                Face::Regular,
                11.0,
                [
                    px + 8.0,
                    preview_y + preview_h * 0.45,
                    preview_w - 16.0,
                    36.0,
                ],
                theme.text_muted,
                if self.default_worker.is_some() {
                    "正在加载默认皮肤…"
                } else {
                    "离线时默认皮肤不可用"
                },
            );
        }
        for ([bx, by, bw, bh], col) in &self.bitmap {
            ui.rounded(
                ox + bx * scale,
                oy + by * scale,
                bw * scale,
                bh * scale,
                0.0,
                *col,
            );
        }

        // Floating camera orbit & zoom dock at the bottom of the 3D preview card
        let control_y = preview_y + preview_h - 36.0;
        let btn_w = 34.0;
        let gap = 6.0;
        let cluster = 2.0 * btn_w + gap;
        let cx = px + (preview_w - cluster) * 0.5;
        for (i, (label, action)) in [
            ("<", Action::WardrobeRotate(-1)),
            (">", Action::WardrobeRotate(1)),
        ]
        .into_iter()
        .enumerate()
        {
            button(
                ui,
                targets,
                [cx + i as f32 * (btn_w + gap), control_y, btn_w, 28.0],
                label,
                action,
                false,
            );
        }

        // Primary quick-toggle button directly below 3D preview card
        let primary_toggle_y = preview_y + preview_h + 8.0;
        if self.capes {
            button(
                ui,
                targets,
                [px, primary_toggle_y, preview_w, 34.0],
                if selection.hide_cape {
                    "披风可见性：隐藏（点击显示）"
                } else {
                    "披风可见性：显示（点击隐藏）"
                },
                Action::WardrobeHideCape,
                !selection.hide_cape,
            );
        } else {
            button(
                ui,
                targets,
                [px, primary_toggle_y, preview_w, 34.0],
                if model == Model::Slim {
                    "手臂模型：Alex / 纤细（3px）"
                } else {
                    "手臂模型：Steve / 经典（4px）"
                },
                Action::WardrobeModel,
                false,
            );
        }

        // 3. Right / Bottom Gallery & Details Column
        let lx = if wide { x + preview_w + 16.0 } else { x };
        let lw = if wide { w - preview_w - 16.0 } else { w };
        let listtop = if wide {
            preview_y + slide_y
        } else {
            primary_toggle_y + 42.0 + slide_y
        };

        // Active Appearance Details & Primary Action Bar
        let active_title = if let Some(id) = selected_id {
            self.library
                .entries
                .iter()
                .find(|e| e.id == id && e.kind == self.kind())
                .map(|e| format!("当前本地：{}", e.title))
                .unwrap_or_else(|| "当前本地覆盖".into())
        } else if self.capes {
            if selection.hide_cape {
                "本地隐藏披风".into()
            } else if let Some(c) = self.cache.snapshot.capes.iter().find(|c| c.active) {
                format!("当前云端披风：{}", c.title)
            } else if self.provider.is_some() {
                "账号云端披风".into()
            } else {
                "未装备披风".into()
            }
        } else if self.provider.is_some() {
            format!("云端档案（{}）", self.name)
        } else {
            "默认外观（Steve / 经典）".into()
        };

        let detail_h = 46.0;
        ui.rounded(lx, listtop, lw, detail_h, 12.0, theme.surface);
        ui.border(lx, listtop, lw, detail_h, 12.0, theme.border);
        let import_w = (lw * 0.42).clamp(118.0, 168.0);
        let info_w = (lw - import_w - 24.0).max(60.0);
        let fitted_active = ui.fit_text(Face::SemiBold, 11.5, &active_title, info_w);
        ui.text(
            Face::SemiBold,
            11.5,
            lx + 12.0,
            listtop + 6.0,
            theme.text_primary,
            &fitted_active,
        );
        let detail_sub = if self.worker.is_some() {
            "正在与账号服务同步…"
        } else if crate::ha_launcher::documents::busy() {
            "等待安卓文件选择器…"
        } else if self.capes {
            "导入 64×32 PNG 披风或从下方选择"
        } else {
            "导入 64×64 PNG 皮肤或从下方选择"
        };
        let fitted_sub = ui.fit_text(Face::Regular, 9.5, detail_sub, info_w);
        ui.text(
            Face::Regular,
            9.5,
            lx + 12.0,
            listtop + 24.0,
            if self.worker.is_some() {
                theme.accent
            } else {
                theme.text_muted
            },
            &fitted_sub,
        );
        button(
            ui,
            targets,
            [lx + lw - import_w - 8.0, listtop + 7.0, import_w, 32.0],
            if self.capes {
                "+ 导入披风 PNG"
            } else {
                "+ 导入皮肤 PNG"
            },
            Action::WardrobeImport,
            true,
        );

        let list_start = listtop + detail_h + 10.0;
        let bottom = h - 42.0;
        let mut row = list_start - scroll;
        let card_h = 42.0;
        let card_step = 48.0;

        let draw_gallery_item = |ui: &mut Ui,
                                 targets: &mut Vec<([f32; 4], Action)>,
                                 ry: f32,
                                 title: &str,
                                 badge: &str,
                                 is_selected: bool,
                                 action: Action| {
            let r = [lx, ry, lw, card_h];
            ui.rounded(
                r[0],
                r[1],
                r[2],
                r[3],
                11.0,
                if is_selected {
                    theme.surface_selected
                } else {
                    theme.surface
                },
            );
            ui.stroke_rounded(
                r[0],
                r[1],
                r[2],
                r[3],
                11.0,
                if is_selected { 1.6 } else { 1.0 },
                if is_selected {
                    theme.accent
                } else {
                    theme.border
                },
            );
            // Leading icon badge
            let ib = [r[0] + 10.0, r[1] + 8.0, 26.0, 26.0];
            ui.rounded(
                ib[0],
                ib[1],
                ib[2],
                ib[3],
                8.0,
                if is_selected {
                    theme.accent
                } else {
                    theme.surface_dim
                },
            );
            if is_selected {
                ui.icon(Icon::Check, ib[0] + 5.0, ib[1] + 5.0, 16.0, theme.on_accent);
            } else {
                ui.icon(
                    Icon::Installed,
                    ib[0] + 5.0,
                    ib[1] + 5.0,
                    16.0,
                    theme.text_muted,
                );
            }
            let max_tw = (lw - 132.0).max(60.0);
            let t_fit = ui.fit_text(Face::SemiBold, 12.0, title, max_tw);
            ui.text(
                Face::SemiBold,
                12.0,
                r[0] + 44.0,
                r[1] + 6.0,
                theme.text_primary,
                &t_fit,
            );
            ui.text(
                Face::Regular,
                9.5,
                r[0] + 44.0,
                r[1] + 23.0,
                if is_selected {
                    theme.accent
                } else {
                    theme.text_muted
                },
                badge,
            );

            let pill_txt = if is_selected { "已启用" } else { "应用" };
            let pill_w = 64.0;
            let pr = [r[0] + lw - pill_w - 10.0, r[1] + 8.0, pill_w, 26.0];
            ui.rounded(
                pr[0],
                pr[1],
                pr[2],
                pr[3],
                8.0,
                if is_selected {
                    theme.accent
                } else {
                    theme.surface_dim
                },
            );
            ui.centered_fit(
                Face::SemiBold,
                10.5,
                pr,
                if is_selected {
                    theme.on_accent
                } else {
                    theme.text_primary
                },
                pill_txt,
            );
            targets.push((r, action));
        };

        let mut shown_items = 0usize;

        // Cloud Capes (when on Capes tab and signed into an account)
        if self.capes && self.provider.is_some() {
            for (index, cape) in self.cache.snapshot.capes.iter().enumerate() {
                shown_items += 1;
                if row >= list_start - 4.0 && row + card_h <= bottom + 4.0 {
                    let is_active = cape.active && selected_id.is_none() && !selection.hide_cape;
                    draw_gallery_item(
                        ui,
                        targets,
                        row,
                        &cape.title,
                        if is_active {
                            "云端披风 · 已装备到档案"
                        } else {
                            "云端披风 · 点击启用"
                        },
                        is_active,
                        Action::WardrobeActivateCape(index),
                    );
                }
                row += card_step;
            }
        }

        // Local Gallery Items (Skins or Capes)
        for (index, entry) in self
            .library
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == self.kind())
        {
            shown_items += 1;
            if row >= list_start - 4.0 && row + card_h <= bottom + 4.0 {
                let is_sel = selected_id == Some(entry.id.as_str());
                draw_gallery_item(
                    ui,
                    targets,
                    row,
                    &entry.title,
                    if is_sel {
                        "本地图库 · 当前已应用"
                    } else if self.capes {
                        "本地披风 PNG · 点击装备"
                    } else {
                        "本地皮肤 PNG · 点击装备"
                    },
                    is_sel,
                    Action::WardrobeSelect(index),
                );
            }
            row += card_step;
        }

        // Empty State Card when gallery has no items for this tab
        if shown_items == 0 {
            if row >= list_start - 4.0 && row + 64.0 <= bottom + 8.0 {
                ui.rounded(lx, row, lw, 60.0, 12.0, theme.surface_dim);
                ui.border(lx, row, lw, 60.0, 12.0, theme.border);
                ui.text(
                    Face::SemiBold,
                    12.0,
                    lx + 14.0,
                    row + 10.0,
                    theme.text_primary,
                    if self.capes {
                        "你的图库中还没有自定义披风"
                    } else {
                        "你的图库中还没有自定义皮肤"
                    },
                );
                let empty_sub = ui.fit_text(
                    Face::Regular,
                    10.0,
                    if self.capes {
                        "点击上方「+ 导入披风 PNG」从设备添加 64×32 披风贴图。"
                    } else {
                        "点击上方「+ 导入皮肤 PNG」从设备添加 64×64 皮肤贴图。"
                    },
                    lw - 28.0,
                );
                ui.text(
                    Face::Regular,
                    10.0,
                    lx + 14.0,
                    row + 31.0,
                    theme.text_muted,
                    &empty_sub,
                );
            }
            row += 68.0;
        }

        // Account & Reset Action Buttons
        if self.provider.is_none() {
            if row >= list_start - 4.0 && row + 34.0 <= bottom + 4.0 {
                button(
                    ui,
                    targets,
                    [lx, row, lw, 34.0],
                    if self.capes {
                        "清除本地披风覆盖"
                    } else {
                        "在本地恢复默认外观"
                    },
                    Action::WardrobeUseCloud,
                    false,
                );
            }
            row += 42.0;
        } else {
            let half_w = (lw - 8.0) * 0.5;
            if row >= list_start - 4.0 && row + 34.0 <= bottom + 4.0 {
                button(
                    ui,
                    targets,
                    [lx, row, half_w, 34.0],
                    "使用云端外观",
                    Action::WardrobeUseCloud,
                    false,
                );
                button(
                    ui,
                    targets,
                    [lx + half_w + 8.0, row, half_w, 34.0],
                    if self.worker.is_some() {
                        "正在同步云端…"
                    } else {
                        "刷新云端账号"
                    },
                    Action::WardrobeCloud,
                    false,
                );
            }
            row += 42.0;
            if !self.capes {
                if row >= list_start - 4.0 && row + 34.0 <= bottom + 4.0 {
                    button(
                        ui,
                        targets,
                        [lx, row, lw, 34.0],
                        if self.provider == Some(Provider::Microsoft) {
                            "将所选皮肤上传到微软账号"
                        } else {
                            "在网站上管理 Ely.by 云端皮肤"
                        },
                        Action::WardrobeUpload,
                        false,
                    );
                }
                row += 42.0;
            }
        }

        self.view = [list_start, bottom, (row + scroll + 38.0 - bottom).max(0.0)];

        // Bottom status / feedback bar
        if !self.note.is_empty() {
            let banner_y = h - 36.0;
            ui.rounded(x, banner_y, w, 28.0, 8.0, theme.surface_dim);
            let note_col = if self.damaged {
                theme.danger
            } else {
                theme.text_muted
            };
            let note = ui.fit_text(Face::Regular, 10.8, &self.note, w - 20.0);
            ui.text(
                Face::Regular,
                10.8,
                x + 10.0,
                banner_y + 6.0,
                note_col,
                &note,
            );
        }
    }
}
