#![windows_subsystem = "windows"]

mod backup;
mod detector;
mod logger;
mod patcher;

use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

fn main() -> eframe::Result<()> {
    logger::init();
    logger::log("Application starting");

    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}\n{:?}", info, std::backtrace::Backtrace::capture());
        logger::log(&msg);
        // Show message box so user sees the error instead of silent crash
        let _ = std::process::Command::new("cmd")
            .args(["/C", "msg", "*", &format!("ClaudeCN 遇到错误，请将日志发给开发者：\n{}", info)])
            .status();
    }));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 420.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Claude 汉化助手",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let font_paths = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
    ];

    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                "chinese".into(),
                Arc::new(egui::FontData::from_owned(data)),
            );
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, "chinese".into());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("chinese".into());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}

fn set_mutex<T>(mutex: &Mutex<T>, val: T) {
    match mutex.lock() {
        Ok(mut guard) => *guard = val,
        Err(e) => *e.into_inner() = val,
    }
}

fn get_mutex<T: Clone>(mutex: &Mutex<T>) -> Option<T> {
    match mutex.lock() {
        Ok(guard) => Some(guard.clone()),
        Err(e) => Some(e.into_inner().clone()),
    }
}

struct App {
    status: detector::PatchStatus,
    installation: Option<detector::ClaudeInstallation>,
    message: Arc<Mutex<String>>,
    is_processing: Arc<Mutex<bool>>,
    needs_refresh: Arc<Mutex<bool>>,
    is_admin: bool,
    show_disclaimer: bool,
}

impl App {
    fn new() -> Self {
        let is_admin = check_admin();
        logger::log(&format!("is_admin: {}", is_admin));
        let installation = if is_admin {
            detector::find_claude()
        } else {
            logger::log("skipping detection: not running as admin");
            None
        };
        let status = match &installation {
            Some(inst) => detector::check_patch_status(inst),
            None => detector::PatchStatus::NotInstalled,
        };

        let message = if is_admin {
            "就绪".to_string()
        } else {
            "请右键以管理员身份运行本程序".to_string()
        };

        let disclaimer_path = std::env::temp_dir().join("claudecn_disclaimer_accepted");
        let show_disclaimer = !disclaimer_path.exists();

        Self {
            status,
            installation,
            message: Arc::new(Mutex::new(message)),
            is_processing: Arc::new(Mutex::new(false)),
            needs_refresh: Arc::new(Mutex::new(false)),
            is_admin,
            show_disclaimer,
        }
    }

    fn refresh_status(&mut self) {
        self.installation = detector::find_claude();
        self.status = match &self.installation {
            Some(inst) => detector::check_patch_status(inst),
            None => detector::PatchStatus::NotInstalled,
        };
    }

    fn run_task(
        &self,
        installation: detector::ClaudeInstallation,
        task: fn(&detector::ClaudeInstallation, &dyn Fn(&str)) -> Result<(), patcher::PatchError>,
        success_msg: &'static str,
        fail_prefix: &'static str,
    ) {
        let msg = self.message.clone();
        let busy = self.is_processing.clone();
        let refresh = self.needs_refresh.clone();

        set_mutex(&busy, true);

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task(&installation, &|s| {
                    set_mutex(&msg, s.to_string());
                })
            }));

            match result {
                Ok(Ok(())) => set_mutex(&msg, success_msg.into()),
                Ok(Err(e)) => set_mutex(&msg, format!("{}: {}", fail_prefix, e)),
                Err(_) => set_mutex(&msg, format!("{}：发生内部错误", fail_prefix)),
            }

            set_mutex(&busy, false);
            set_mutex(&refresh, true);
        });
    }
}

fn check_admin() -> bool {
    std::process::Command::new("net")
        .args(["session"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_busy = get_mutex(&self.is_processing).unwrap_or(false);

        if is_busy {
            ctx.request_repaint();
        }

        if get_mutex(&self.needs_refresh).unwrap_or(false) {
            self.refresh_status();
            set_mutex(&self.needs_refresh, false);
        }

        if self.show_disclaimer {
            egui::Window::new("免费声明")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.label("本软件由「Winhao学AI」（抖音号：54927876676）开发，完全免费。");
                    ui.add_space(8.0);
                    ui.label("严禁任何形式的商业使用，包括但不限于：");
                    ui.label("  • 出售本软件或其修改版本");
                    ui.label("  • 将本软件作为付费服务的一部分");
                    ui.label("  • 利用本软件进行任何商业盈利活动");
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("如果你是付费获得本软件的，说明你被骗了！请立即举报卖家。")
                            .color(egui::Color32::from_rgb(220, 80, 80)),
                    );
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        if ui.button("我知道了，免费使用").clicked() {
                            self.show_disclaimer = false;
                            let path = std::env::temp_dir().join("claudecn_disclaimer_accepted");
                            let _ = std::fs::write(&path, "accepted");
                        }
                    });
                    ui.add_space(4.0);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.heading("Claude 桌面端汉化助手");
                ui.label(
                    egui::RichText::new("v1.2.0")
                        .size(12.0)
                        .color(egui::Color32::GRAY),
                );
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);

            if !self.is_admin {
                ui.label(
                    egui::RichText::new("⚠ 请右键以管理员身份运行本程序")
                        .color(egui::Color32::from_rgb(220, 80, 80))
                        .size(14.0),
                );
                ui.add_space(12.0);
            }

            egui::Grid::new("status_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("安装状态:");
                    match &self.installation {
                        Some(inst) => {
                            ui.label(
                                egui::RichText::new(format!("已检测到  v{}", inst.version))
                                    .color(egui::Color32::from_rgb(80, 200, 120)),
                            );
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("未检测到 Claude Desktop")
                                    .color(egui::Color32::from_rgb(220, 80, 80)),
                            );
                        }
                    }
                    ui.end_row();

                    ui.label("汉化状态:");
                    match self.status {
                        detector::PatchStatus::NotInstalled => {
                            ui.label(egui::RichText::new("—").color(egui::Color32::GRAY));
                        }
                        detector::PatchStatus::Unpatched => {
                            ui.label(
                                egui::RichText::new("未汉化")
                                    .color(egui::Color32::from_rgb(230, 180, 50)),
                            );
                        }
                        detector::PatchStatus::Patched => {
                            ui.label(
                                egui::RichText::new("已汉化")
                                    .color(egui::Color32::from_rgb(80, 200, 120)),
                            );
                        }
                    }
                    ui.end_row();
                });

            ui.add_space(24.0);

            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    let btn_size = egui::vec2(160.0, 40.0);

                    let can_patch = self.is_admin
                        && self.installation.is_some()
                        && self.status != detector::PatchStatus::Patched
                        && !is_busy;

                    if ui
                        .add_enabled(can_patch, egui::Button::new("一键汉化").min_size(btn_size))
                        .clicked()
                    {
                        if let Some(inst) = &self.installation {
                            self.run_task(
                                inst.clone(),
                                patcher::apply_patch,
                                "汉化成功！Claude 已重启。",
                                "汉化失败",
                            );
                        }
                    }

                    ui.add_space(12.0);

                    let can_restore = self.is_admin
                        && self.installation.is_some()
                        && self.status == detector::PatchStatus::Patched
                        && !is_busy;

                    if ui
                        .add_enabled(
                            can_restore,
                            egui::Button::new("一键恢复").min_size(btn_size),
                        )
                        .clicked()
                    {
                        if let Some(inst) = &self.installation {
                            self.run_task(
                                inst.clone(),
                                patcher::remove_patch,
                                "已恢复为英文版本！",
                                "恢复失败",
                            );
                        }
                    }
                });
            });

            ui.add_space(16.0);

            ui.vertical_centered(|ui| {
                if ui
                    .add_enabled(!is_busy, egui::Button::new("刷新状态"))
                    .clicked()
                {
                    self.refresh_status();
                    set_mutex(&self.message, "状态已刷新".into());
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);

            let msg = get_mutex(&self.message).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("状态:");
                if is_busy {
                    ui.spinner();
                }
                ui.label(&msg);
            });

            ui.add_space(4.0);

            ui.separator();
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("作者：Winhao学AI（抖音号：54927876676）").size(11.0));
                ui.label(
                    egui::RichText::new("本软件完全免费，不可商业化")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(230, 180, 50)),
                );
                ui.label(
                    egui::RichText::new("付费获取即被骗，请举报")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(220, 80, 80)),
                );
            });
            ui.add_space(4.0);

            let lines = logger::recent_lines();
            if !lines.is_empty() {
                ui.collapsing("运行日志（点击展开，可复制）", |ui| {
                    let mut log_text = lines.join("\n");
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut log_text)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                });
            }
        });
    }
}
