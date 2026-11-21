//! Explicit, scrollable selections. Values never cycle on repeated button taps.
use super::{
    Launcher,
    pages::{Action, button_rounded},
};
use crate::ha_launcher::catalog::RendererMode;
use crate::ha_launcher::client::android_display::DisplayOption;
use crate::ha_launcher::client::ui_theme::{Face, Ui, color::with_alpha};

pub(super) enum Kind {
    Display(DisplayOption),
    DisplayHz,
    CacheBudget,
    WorkerLimit,
    Renderer,
    RenderScale,
    Profile,
    InputHost,
    UiScale,
    ControlProfile,
}
pub(super) struct Chooser {
    title: &'static str,
    help: &'static str,
    device: Option<String>,
    choices: Vec<(String, String, Action, bool)>,
    /// Starling: a Delete at the right of a choice (a profile's), by its index.
    deletes: Vec<Option<Action>>,
    pub scroll: f32,
}
impl Launcher {
    pub(super) fn open_chooser(&mut self, kind: Kind) {
        crate::ha_launcher::platform::hide();
        self.catalog.editing = None;
        let device = if matches!(kind, Kind::Renderer) {
            self.renderer.as_ref().map(|r| {
                format!(
                    "GPU: {} / {} {}",
                    r.info.name, r.info.driver, r.info.driver_info
                )
            })
        } else {
            None
        };
        let c = &self.catalog.config;
        let mut deletes = Vec::new();
        let (title, help, choices) = match kind {
            Kind::Display(option) => {
                let current = option.value(&c.game_settings(&self.home));
                (
                    option.name(),
                    "已为所选游戏实例保存。",
                    (0..=option.max())
                        .map(|value| (
                            option.choice_name(value).into(),
                            String::new(),
                            Action::ChooseDisplay(option, value),
                            current == value,
                        ))
                        .collect(),
                )
            }
            Kind::CacheBudget => (
                "可复用内存缓存",
                "已解码声音（80%）与空网格缓冲（20%），会以同等质量重建。更小的预算会增加 CPU 负担。并非总内存/显存上限，下次启动游戏时生效。",
                [0, 16, 32, 64, 128, 256]
                    .into_iter()
                    .map(|mib| {
                        (
                            if mib == 0 {
                                "引擎默认（80 MiB）".into()
                            } else {
                                format!("{mib} MiB")
                            },
                            String::new(),
                            Action::ChooseCacheBudget(mib),
                            c.cache_budget_mib == mib,
                        )
                    })
                    .collect(),
            ),
            Kind::WorkerLimit => (
                "区块工作线程上限",
                "仅限世界生成与网格构建线程的上限，不限制所有应用线程。较低的值可减少 CPU 争抢但会拖慢加载，下次启动游戏时生效。",
                (0..=64u32)
                    .map(|limit| {
                        (
                            if limit == 0 {
                                "无额外限制".into()
                            } else {
                                format!("{limit} worker(s)")
                            },
                            String::new(),
                            Action::ChooseWorkerLimit(limit),
                            c.worker_limit == limit,
                        )
                    })
                    .collect(),
            ),
            Kind::DisplayHz => (
                "显示刷新率",
                "这是安卓显示偏好，不是游戏帧率上限。不支持的刷新率可能被忽略。",
                crate::ha_launcher::device_preferences::DISPLAY_RATES
                    .into_iter()
                    .map(|hz| {
                        (
                            if hz == 0 {
                                "系统默认".into()
                            } else {
                                format!("{hz} Hz")
                            },
                            String::new(),
                            Action::ChooseDisplayHz(hz),
                            c.device.display_hz == hz,
                        )
                    })
                    .collect(),
            ),
            Kind::RenderScale => {
                let current = c.game_render_scale(&self.home).percent();
                let mut choices: Vec<_> = crate::ha_launcher::display::RenderScale::PRESETS
                    .into_iter()
                    .map(|percent| {
                        (
                            format!("{percent}%"),
                            if percent == 100 {
                                "原生分辨率".into()
                            } else if percent > 100 {
                                "更高的 3D 渲染分辨率".into()
                            } else {
                                "更低的 3D 渲染分辨率".into()
                            },
                            Action::ChooseRenderScale(crate::ha_launcher::display::RenderScale::new(percent)),
                            current == percent,
                        )
                    })
                    .collect();
                choices.push((
                    "自定义百分比…".into(),
                    "输入 25 到 200 之间的值".into(),
                    Action::Field("render_scale"),
                    false,
                ));
                (
                    "客户端分辨率",
                    "小屏保持可用的菜单尺寸。",
                    choices,
                )
            }
            Kind::InputHost => (
                "输入模式",
                "鼠标模式使用实体键盘，触摸模式保留安卓键盘。",
                crate::ha_launcher::catalog::InputHost::ALL
                    .into_iter()
                    .map(|mode| {
                        (
                            mode.label().into(),
                            String::new(),
                            Action::ChooseInputHost(mode),
                            c.input_host == mode,
                        )
                    })
                    .collect(),
            ),
            Kind::Renderer => (
                "渲染模式",
                "使用系统 GPU 驱动，将在下次启动游戏时生效。",
                vec![
                    (
                        "OpenGL ES / 实验性".into(),
                        "适用于较旧的 GPU，需要 ES 3.0+ 与足够的驱动限制。".into(),
                        Action::ChooseRenderer(RendererMode::OpenGl),
                        c.renderer == RendererMode::OpenGl,
                    ),
                    (
                        "Vulkan / GPU 批处理".into(),
                        "优化的 Vulkan 管线，按页批处理绘制并高速流式加载区块。".into(),
                        Action::ChooseRenderer(RendererMode::Vulkan),
                        c.renderer == RendererMode::Vulkan,
                    ),
                    (
                        "Vulkan / 兼容".into(),
                        "标准 Vulkan 渲染路径，按区域直接绘制。"
                            .into(),
                        Action::ChooseRenderer(RendererMode::VulkanCompatibility),
                        c.renderer == RendererMode::VulkanCompatibility,
                    ),
                ],
            ),
            Kind::Profile => {
                let mut ids = vec![0, c.profile];
                ids.extend(
                    std::fs::read_dir(self.home.join("profiles"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|e| e.path().is_dir())
                        .filter_map(|e| {
                            e.file_name()
                                .to_str()?
                                .strip_prefix("profile-")?
                                .parse::<u32>()
                                .ok()
                        }),
                );
                ids.sort_unstable();
                ids.dedup();
                let next = ids.last().copied().unwrap_or(0).checked_add(1);
                // Every profile but the game folder itself (0) can be deleted.
                deletes = ids
                    .iter()
                    .map(|&id| (id != 0).then_some(Action::DeleteProfile(id)))
                    .collect();
                let mut choices: Vec<_> = ids
                    .into_iter()
                    .map(|id| {
                        (
                            // As the Play page counts them: Profile 1 is the game folder (0).
                            format!("档案 {}", id + 1),
                            if id == 0 {
                                "默认档案".into()
                            } else {
                                "独立的存档、模组与整合包".into()
                            },
                            Action::ChooseProfile(id),
                            id == c.profile,
                        )
                    })
                    .collect();
                if let Some(id) = next {
                    choices.push((
                        "新建档案".into(),
                        format!("创建档案 {}", id + 1),
                        Action::ChooseProfile(id),
                        false,
                    ));
                }
                (
                    "选择档案",
                    "每个档案拥有独立的游戏设置与存档。",
                    choices,
                )
            }
            Kind::UiScale => (
                "启动器界面缩放",
                "让启动器适配手机 DPI。若触摸键和文字感觉太小或太大，可调大或调小。",
                [
                    (
                        0u32,
                        "自动（手机 DPI）",
                        "自动匹配显示器的 DPI 密度",
                    ),
                    (85, "85%（紧凑）", "屏幕上可容纳更多行"),
                    (100, "100%（默认 DPI）", "标准手机 DPI 缩放"),
                    (115, "115%（大）", "更大的按钮、卡片与文字"),
                    (
                        130,
                        "130%（特大）",
                        "高 DPI 手机最大触摸舒适度",
                    ),
                ]
                .into_iter()
                .map(|(pct, title, desc)| {
                    (
                        title.into(),
                        desc.into(),
                        Action::ChooseUiScale(pct),
                        c.ui_scale_percent == pct,
                    )
                })
                .collect(),
            ),
            Kind::ControlProfile => {
                let game_home = c.game_home(&self.home);
                let touch = crate::ha_launcher::client::touch::Settings::load_from(
                    &game_home.join("horizon-touch.json"),
                );
                let style = touch.control_style;
                let presets = crate::ha_launcher::client::touch::Settings::list_presets(&game_home, style);
                let choices = presets
                    .into_iter()
                    .enumerate()
                    .map(|(idx, meta)| {
                        let active = touch.preset_id.eq_ignore_ascii_case(&meta.id);
                        let mut title = meta.name.clone();
                        if meta.is_default {
                            title.push_str("  [受保护的默认]");
                        }
                        let sub = format!(
                            "{}  •  controls/{}/{}.json",
                            meta.description,
                            style.storage_subdir(),
                            meta.id
                        );
                        (title, sub, Action::ChooseControlProfile(idx as u32), active)
                    })
                    .collect();
                (
                    match style {
                        crate::ha_launcher::client::touch::ControlStyle::Mcpe => "MCPE 按键预设",
                        crate::ha_launcher::client::touch::ControlStyle::GoyControl => {
                            "GoyControl 按键预设"
                        }
                    },
                    match style {
                        crate::ha_launcher::client::touch::ControlStyle::Mcpe => {
                            "独立的 MCPE 预设存储于 controls/mcpe/，默认预设受保护。"
                        }
                        crate::ha_launcher::client::touch::ControlStyle::GoyControl => {
                            "独立的 GoyControl 预设存储于 controls/goycontrol/，默认预设受保护。"
                        }
                    },
                    choices,
                )
            }
        };
        self.chooser = Some(Chooser {
            title,
            help,
            choices,
            deletes,
            device,
            scroll: 0.0,
        });
    }
}
impl Chooser {
    pub(super) fn confirm_delete(name: String) -> Self {
        Self {
            title: "移除附加项？",
            help: "仅从此档案中删除所选文件，此操作不可撤销。",
            device: Some(name),
            choices: vec![
                (
                    "移除".into(),
                    String::new(),
                    Action::ConfirmDeleteLocal,
                    false,
                ),
                ("取消".into(), String::new(), Action::Dismiss, true),
            ],
            deletes: Vec::new(),
            scroll: 0.0,
        }
    }
    /// Starling: deleting profile `id` (its folder: worlds, mods, packs, settings) asks first;
    /// Cancel goes back to the profiles.
    pub(super) fn confirm_delete_profile(id: u32) -> Self {
        Self {
            title: "删除档案？",
            help: "将删除其世界、模组、整合包与设置，且无法撤销。",
            device: Some(format!("档案 {}", id + 1)),
            choices: vec![
                (
                    "删除".into(),
                    format!("彻底删除档案 {}", id + 1),
                    Action::ConfirmDeleteProfile(id),
                    false,
                ),
                (
                    "取消".into(),
                    "返回档案".into(),
                    Action::Profile,
                    true,
                ),
            ],
            deletes: Vec::new(),
            scroll: 0.0,
        }
    }
    pub(super) fn navigation(&self, width: f32) -> Vec<([f32; 4], Action)> {
        self.choices
            .iter()
            .enumerate()
            .map(|(i, (_, _, action, _))| {
                (
                    [
                        32.0,
                        137.0 + i as f32 * 84.0 - self.scroll,
                        width - 64.0,
                        76.0,
                    ],
                    *action,
                )
            })
            .collect()
    }
    pub(super) fn viewport(&self, height: f32) -> [f32; 3] {
        [
            137.0,
            height - 100.0,
            (self.choices.len() as f32 * 84.0 - (height - 237.0)).max(0.0),
        ]
    }
    pub fn draw(&mut self, ui: &mut Ui, targets: &mut Vec<([f32; 4], Action)>, modal_anim: f32) {
        let (w, h) = (ui.w, ui.h);
        let theme = crate::ha_launcher::client::ui_theme::active_theme();
        let slide = (1.0 - modal_anim.clamp(0.0, 1.0)) * 16.0;
        targets.clear();
        ui.draw_theme_background(0.0, 0.0, w, h, &theme);
        ui.text(
            Face::SemiBold,
            24.0,
            32.0,
            32.0 + slide,
            theme.text_primary,
            self.title,
        );
        ui.text(
            Face::Regular,
            14.0,
            32.0,
            74.0 + slide,
            theme.text_muted,
            self.help,
        );
        if let Some(device) = &self.device {
            ui.hint_line(32.0, 100.0 + slide, w - 64.0, device, theme.accent);
        }
        let top = 132.0 + slide;
        let bottom = h - 94.0;
        self.scroll = self.scroll.clamp(
            0.0,
            (self.choices.len() as f32 * 84.0 - (bottom - top)).max(0.0),
        );
        ui.set_clip(Some([0.0, top, w, (bottom - top).max(1.0)]));
        for (i, (label, detail, action, selected)) in self.choices.iter().enumerate() {
            let y = top + i as f32 * 84.0 - self.scroll;
            if y + 76.0 < top || y > bottom {
                continue;
            }
            ui.rounded(
                32.0,
                y,
                w - 64.0,
                74.0,
                10.0,
                if *selected {
                    theme.surface_selected
                } else {
                    theme.surface
                },
            );
            ui.stroke_rounded(
                32.0,
                y,
                w - 64.0,
                74.0,
                10.0,
                if *selected { 1.6 } else { 1.0 },
                if *selected {
                    theme.accent
                } else {
                    theme.border
                },
            );
            ui.stroke_rounded(
                48.0,
                y + 26.0,
                22.0,
                22.0,
                11.0,
                1.8,
                if *selected {
                    theme.accent
                } else {
                    theme.text_muted
                },
            );
            if *selected {
                ui.rounded(53.0, y + 31.0, 12.0, 12.0, 6.0, theme.accent);
            }
            ui.text(
                Face::SemiBold,
                17.0,
                84.0,
                y + 11.0,
                theme.text_primary,
                label,
            );
            ui.text(
                Face::Regular,
                13.0,
                84.0,
                y + 40.0,
                theme.text_muted,
                detail,
            );
            if let Some(c) = ui.clip_rect([32.0, y, w - 64.0, 74.0]) {
                targets.push((c, *action));
            }
            // After the card's own target: the last one under a finger wins.
            if let Some(delete) = self.deletes.get(i).copied().flatten() {
                let r = [w - 48.0 - 96.0, y + 17.0, 96.0, 40.0];
                ui.rounded(r[0], r[1], r[2], r[3], 10.0, with_alpha(theme.danger, 0.16));
                ui.border(r[0], r[1], r[2], r[3], 10.0, theme.danger);
                ui.centered_fit(Face::Medium, 14.0, r, theme.danger, "删除");
                if let Some(c) = ui.clip_rect(r) {
                    targets.push((c, delete));
                }
            }
        }
        ui.set_clip(None);
        button_rounded(
            ui,
            targets,
            [32.0, h - 74.0, 160.0, 44.0],
            "取消",
            Action::Dismiss,
            false,
            10.0,
        );
    }
}
