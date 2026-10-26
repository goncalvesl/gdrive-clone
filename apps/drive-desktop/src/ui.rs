use crate::dialog::{self, Target};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText, Vec2};
use engine::{Paths, Report};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

const BLUE: Color32 = Color32::from_rgb(26, 115, 232);
const BORDER: Color32 = Color32::from_rgb(218, 220, 224);
const SURFACE: Color32 = Color32::from_rgb(248, 249, 250);
const MUTED: Color32 = Color32::from_rgb(95, 99, 104);
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Activity,
    Drive,
    Computer,
    Advanced,
}
enum Event {
    Report(Report, bool),
    Message(String),
    History(Vec<String>),
    Connected,
    AccountInfo(drive_api::AccountInfo),
    Picked(Target, Option<PathBuf>),
    Folders(Vec<(String, String)>, Vec<(String, String)>),
}
#[derive(Clone, PartialEq)]
struct Setup {
    local: String,
    remote: String,
    oauth: String,
    deletions: bool,
}
struct WorkerFailure {
    message: String,
    retry_minimum: Option<Duration>,
}
struct App {
    paths: Paths,
    tab: Tab,
    local: String,
    remote: String,
    remote_name: String,
    folder_trail: Vec<(String, String)>,
    folders_loaded: bool,
    show_folders: bool,
    saved: Option<Setup>,
    oauth: String,
    deletions: bool,
    message: String,
    report: Option<Report>,
    history: Vec<String>,
    folders: Vec<(String, String)>,
    worker: Option<Receiver<Result<Event, WorkerFailure>>>,
    progress_rx: Option<Receiver<drive_api::TransferProgress>>,
    transfer: Option<drive_api::TransferProgress>,
    account_info: Option<drive_api::AccountInfo>,
    auto: bool,
    retry_failures: u32,
    next_cycle: Instant,
    preview: bool,
    screenshot: Option<PathBuf>,
    frames: usize,
    screenshot_requested: bool,
}
#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum ScreenshotPage {
    #[default]
    Drive,
    Computer,
    Settings,
}
pub fn launch(paths: Paths, screenshot: Option<PathBuf>, page: ScreenshotPage) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([940., 720.])
            .with_min_inner_size([780., 600.])
            .with_active(screenshot.is_none()),
        ..Default::default()
    };
    eframe::run_native(
        "Preferências — Drive para Linux",
        options,
        Box::new(move |cc| {
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = Color32::WHITE;
            visuals.indent_has_left_vline = false;
            visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
            visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(241, 246, 254);
            visuals.window_fill = Color32::WHITE;
            visuals.override_text_color = Some(Color32::from_rgb(60, 64, 67));
            visuals.selection.bg_fill = Color32::from_rgb(232, 240, 254);
            visuals.selection.stroke = egui::Stroke::new(1_f32, BLUE);
            visuals.widgets.inactive.bg_fill = Color32::WHITE;
            visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1_f32, BORDER);
            visuals.widgets.hovered.bg_fill = Color32::from_rgb(241, 246, 254);
            visuals.hyperlink_color = BLUE;
            cc.egui_ctx.set_visuals(visuals);
            let mut style = (*cc.egui_ctx.style()).clone();
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
            style
                .text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(12.));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.));
            style.spacing.item_spacing = Vec2::new(8., 5.);
            style.spacing.button_padding = Vec2::new(14., 8.);
            cc.egui_ctx.set_style(style);
            let capture = screenshot.is_some();
            let mut app = App::new(paths, screenshot);
            if capture {
                app.tab = match page {
                    ScreenshotPage::Drive => Tab::Drive,
                    ScreenshotPage::Computer => Tab::Computer,
                    ScreenshotPage::Settings => Tab::Advanced,
                };
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Não foi possível abrir a interface: {e}"))
}
impl App {
    fn new(paths: Paths, screenshot: Option<PathBuf>) -> Self {
        let config = paths.load().ok();
        let account = paths.account().ok();
        let saved = config.as_ref().map(|c| Setup {
            local: c.local.display().to_string(),
            remote: c.remote_id.clone(),
            oauth: c.credentials.display().to_string(),
            deletions: c.propagate_deletions,
        });
        Self {
            local: config
                .as_ref()
                .map(|c| c.local.display().to_string())
                .unwrap_or_default(),
            remote: config
                .as_ref()
                .map(|c| c.remote_id.clone())
                .unwrap_or_default(),
            remote_name: config
                .as_ref()
                .and_then(|c| c.remote_name.clone())
                .unwrap_or_else(|| "Pasta configurada".into()),
            folder_trail: vec![("root".into(), "Meu Drive".into())],
            folders_loaded: false,
            show_folders: false,
            saved,
            oauth: paths
                .application_credentials()
                .ok()
                .map(|p| p.display().to_string())
                .or_else(|| {
                    account
                        .as_ref()
                        .map(|a| a.credentials.display().to_string())
                        .or_else(|| config.as_ref().map(|c| c.credentials.display().to_string()))
                })
                .unwrap_or_default(),
            deletions: config.as_ref().is_some_and(|c| c.propagate_deletions),
            tab: Tab::Drive,
            paths,
            message: "Escolha as pastas e salve suas preferências.".into(),
            report: None,
            history: Vec::new(),
            folders: Vec::new(),
            worker: None,
            progress_rx: None,
            transfer: None,
            account_info: None,
            auto: false,
            retry_failures: 0,
            next_cycle: Instant::now(),
            preview: false,
            screenshot,
            frames: 0,
            screenshot_requested: false,
        }
    }

    fn render_preferences(&mut self, ctx: &egui::Context) {
        let before = self.setup();
        egui::TopBottomPanel::top("preferences-header")
            .exact_height(72.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(22, 16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    app_symbol(ui);
                    ui.label(RichText::new("Drive para Linux").size(22.).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.worker.is_some() && self.progress_rx.is_some() && ui.button("Pausar").clicked() {
                            self.auto = false;
                            match self.paths.set_paused(true) {
                                Ok(()) => self.message = "Pausa solicitada; o arquivo atual será concluído antes de parar.".into(),
                                Err(error) => self.message = format!("{error:#}"),
                            }
                        }
                        let has_account = self.paths.account().is_ok();
                        if has_account {
                            if header_button(
                                ui,
                                HeaderIcon::Account,
                                self.account_info
                                    .as_ref()
                                    .and_then(|info| info.email.as_deref())
                                    .unwrap_or("Conta Google"),
                            )
                            .clicked()
                            {
                                self.tab = Tab::Advanced;
                            }
                        } else if ui.add_enabled(self.worker.is_none(), primary_button("Fazer login")).clicked() {
                            if self.oauth.is_empty() {
                                self.tab = Tab::Advanced;
                                self.message =
                                    "O login ainda não está disponível nesta compilação.".into();
                            } else {
                                self.start_login();
                            }
                        }
                        if header_button(ui, HeaderIcon::Settings, "Configurações").clicked() {
                            self.tab = Tab::Advanced;
                        }
                        if header_button(ui, HeaderIcon::Help, "Ajuda").clicked() {
                            match Command::new("xdg-open")
                                .arg("https://support.google.com/drive/answer/13401938?hl=pt-BR")
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .spawn()
                            {
                                Ok(_) => {}
                                Err(error) => self.message = error.to_string(),
                            }
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("preferences-footer")
            .min_height(72.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(22, 12))
                    .stroke(egui::Stroke::new(1_f32, BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 230.).max(220.));
                        if let Some(transfer) = &self.transfer {
                            ui.add(
                                egui::Label::new(format!(
                                    "{} {}",
                                    if transfer.upload {
                                        "Enviando"
                                    } else {
                                        "Recebendo"
                                    },
                                    transfer.path
                                ))
                                .truncate(),
                            );
                            let fraction = if transfer.total == 0 {
                                0.
                            } else {
                                (transfer.bytes as f64 / transfer.total as f64).clamp(0., 1.) as f32
                            };
                            ui.add(egui::ProgressBar::new(fraction).desired_width(320.).text(
                                format!(
                                    "{} / {}",
                                    format_bytes(transfer.bytes),
                                    format_bytes(transfer.total)
                                ),
                            ));
                            if transfer.bytes >= transfer.total {
                                ui.label(
                                    RichText::new("Aguardando validação e confirmação…")
                                        .small()
                                        .color(MUTED),
                                );
                            }
                        } else {
                            ui.label(RichText::new(&self.message).small().color(MUTED));
                        }
                        if self.worker.is_some() {
                            ui.spinner();
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can_save = self.worker.is_none()
                            && self.dirty()
                            && !self.remote.is_empty()
                            && !self.local.is_empty()
                            && !self.oauth.is_empty();
                        if ui.add_enabled(can_save, primary_button("Salvar")).clicked() {
                            self.save_setup();
                        }
                        if ui
                            .add_enabled(
                                self.worker.is_none() && self.dirty(),
                                egui::Button::new("Cancelar"),
                            )
                            .clicked()
                        {
                            self.discard_setup();
                        }
                    });
                });
            });
        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(210.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(0, 10)),
            )
            .show(ctx, |ui| {
                if navigation_item(
                    ui,
                    self.tab == Tab::Computer,
                    "Meu computador",
                    "Pastas do computador",
                )
                .clicked()
                {
                    self.tab = Tab::Computer;
                }
                ui.add_space(4.);
                if navigation_item(
                    ui,
                    self.tab == Tab::Drive,
                    "Google Drive",
                    "Pastas do Google Drive",
                )
                .clicked()
                {
                    self.tab = Tab::Drive;
                }
                ui.add_space(40.);
                ui.indent("secondary-navigation", |ui| {
                    if ui
                        .selectable_label(self.tab == Tab::Overview, "Sincronização")
                        .clicked()
                    {
                        self.tab = Tab::Overview;
                    }
                    if ui
                        .selectable_label(self.tab == Tab::Activity, "Atividade")
                        .clicked()
                    {
                        self.tab = Tab::Activity;
                    }
                });
                if let Some(info) = &self.account_info {
                    ui.add_space(36.);
                    ui.indent("storage-usage", |ui| {
                        ui.label(RichText::new("Armazenamento").color(MUTED)).on_hover_text("Uso total informado pelo Google; pode incluir outros serviços e armazenamento compartilhado da organização.");
                        if let Some(usage) = info.usage {
                            if let Some(limit) = info.limit {
                                let fraction = if limit == 0 {
                                    0.
                                } else {
                                    (usage as f64 / limit as f64).clamp(0., 1.) as f32
                                };
                                ui.add(egui::ProgressBar::new(fraction).desired_width(160.));
                                ui.label(
                                    RichText::new(format!(
                                        "{} de {}",
                                        format_bytes(usage),
                                        format_bytes(limit)
                                    ))
                                    .small(),
                                );
                            } else {
                                ui.label(
                                    RichText::new(format!("{} usados", format_bytes(usage)))
                                        .small(),
                                );
                                ui.label(
                                    RichText::new("Limite não informado").small().color(MUTED),
                                );
                            }
                        }
                    });
                }
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 24,
                        top: 8,
                        bottom: 16,
                    }),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(self.worker.is_none(), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("preferences-scroll")
                        .show(ui, |ui| match self.tab {
                            Tab::Drive => self.drive_preferences(ui),
                            Tab::Computer => self.computer_preferences(ui),
                            Tab::Advanced => self.advanced_preferences(ui),
                            Tab::Overview => self.overview(ui),
                            Tab::Activity => {
                                ui.heading("Atividade");
                                ui.label(
                                    RichText::new("Últimas operações concluídas").color(MUTED),
                                );
                                ui.add_space(16.);
                                if ui.button("Atualizar histórico").clicked() {
                                    self.spawn(|paths| Ok(Event::History(paths.history()?)));
                                }
                                if self.history.is_empty() {
                                    ui.label("Nenhuma atividade carregada.");
                                }
                                for item in &self.history {
                                    ui.label(item);
                                }
                            }
                        });
                });
            });
        if before != self.setup() {
            self.auto = false;
        }
        self.folder_dialog(ctx);
    }
    fn setup(&self) -> Setup {
        Setup {
            local: self.local.clone(),
            remote: self.remote.clone(),
            oauth: self.oauth.clone(),
            deletions: self.deletions,
        }
    }
    fn dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.setup())
    }
    fn browse(&mut self, trail: Vec<(String, String)>) {
        let Some((parent, _)) = trail.last() else {
            return;
        };
        let parent = parent.clone();
        self.spawn(move |paths| Ok(Event::Folders(trail, paths.folders(&parent)?)));
    }
    fn spawn(&mut self, job: impl FnOnce(Paths) -> Result<Event> + Send + 'static) {
        if self.worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let paths = self.paths.clone();
        match std::thread::Builder::new()
            .name("drive-worker".into())
            .spawn(move || {
                let result = job(paths).map_err(|e| WorkerFailure {
                    message: format!("{e:#}"),
                    retry_minimum: drive_api::retry_minimum(&e),
                });
                let _ = tx.send(result);
            }) {
            Ok(_) => {
                self.worker = Some(rx);
                self.message = "Processando…".into();
            }
            Err(e) => self.message = format!("Não foi possível iniciar a operação: {e}"),
        }
    }
    fn sync(&mut self, dry: bool) {
        if self.worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let sink: drive_api::ProgressSink = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        self.progress_rx = Some(rx);
        self.transfer = None;
        self.spawn(move |paths| {
            Ok(Event::Report(
                paths.cycle_with_progress(dry, Some(sink))?,
                dry,
            ))
        });
    }
    fn poll(&mut self) {
        if let Some(rx) = &self.progress_rx {
            for progress in rx.try_iter() {
                self.transfer = Some(progress);
            }
        }
        let received = self.worker.as_ref().map(|r| r.try_recv());
        if let Some(result) = received {
            match result {
                Ok(event) => {
                    self.worker = None;
                    self.progress_rx = None;
                    self.transfer = None;
                    let mut next_delay = Duration::from_secs(30);
                    match event {
                        Ok(Event::Report(report, preview)) => {
                            self.retry_failures = 0;
                            self.message = if report.paused {
                                self.auto = false;
                                "Sincronização pausada; operações concluídas foram preservadas."
                                    .into()
                            } else if preview {
                                "Prévia concluída; nenhum arquivo alterado.".into()
                            } else {
                                format!(
                                    "Ciclo concluído: {} operações, {} conflitos, {} bloqueios.",
                                    report.completed + report.recovered,
                                    report.conflicts.len(),
                                    report.blocked.len()
                                )
                            };
                            self.report = Some(report);
                            self.preview = preview;
                        }
                        Ok(Event::Message(message)) => self.message = message,
                        Ok(Event::History(items)) => {
                            self.history = items;
                            self.message = "Histórico carregado.".into();
                        }
                        Ok(Event::Picked(target, selected)) => {
                            if let Some(path) = selected {
                                match target {
                                    Target::Local => self.local = path.display().to_string(),
                                    Target::Credentials => self.oauth = path.display().to_string(),
                                }
                                self.auto = false;
                                self.message = "Seleção atualizada. Salve a configuração antes de sincronizar.".into();
                            } else {
                                self.message = "Seleção cancelada.".into();
                            }
                        }
                        Ok(Event::AccountInfo(info)) => {
                            self.account_info = Some(info);
                            self.message = "Dados da conta atualizados.".into();
                        }
                        Ok(Event::Connected) => {
                            self.message = "Conta conectada. Carregando pastas…".into();
                            self.show_folders = true;
                            self.browse(vec![("root".into(), "Meu Drive".into())]);
                        }
                        Ok(Event::Folders(trail, items)) => {
                            self.folder_trail = trail;
                            self.folders = items;
                            self.folders_loaded = true;
                            self.message = "Abra as subpastas ou escolha a pasta exibida. Depois salve e execute uma prévia.".into();
                            if self.account_info.is_none() {
                                self.spawn(|paths| Ok(Event::AccountInfo(paths.account_info()?)));
                            }
                        }
                        Err(error) => {
                            if self.auto
                                && !self.dirty()
                                && let Some(minimum) = error.retry_minimum
                            {
                                self.retry_failures = self.retry_failures.saturating_add(1);
                                next_delay = engine::retry_delay(self.retry_failures, minimum);
                                self.message = format!(
                                    "{} Nova tentativa de sincronização em {} segundos.",
                                    error.message,
                                    next_delay.as_secs()
                                );
                            } else {
                                self.message = error.message;
                                self.auto = false;
                                self.retry_failures = 0;
                            }
                        }
                    }
                    if let Some(next) = Instant::now().checked_add(next_delay) {
                        self.next_cycle = next;
                    } else {
                        self.auto = false;
                        self.message = "Espera de retry fora do intervalo suportado; sincronização automática interrompida.".into();
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.progress_rx = None;
                    self.transfer = None;
                    self.auto = false;
                    self.message =
                        "A operação terminou inesperadamente. Consulte o journal antes de repetir."
                            .into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    fn overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Seus arquivos, também no Linux");
        ui.label(RichText::new("Espelhamento de uma pasta do Google Drive").color(MUTED));
        ui.add_space(20.);
        egui::Frame::group(ui.style()).inner_margin(20.).show(ui,|ui|{
            ui.horizontal(|ui|{ui.label(RichText::new(if self.worker.is_some(){"● EM ANDAMENTO"}else if self.auto{"● AUTOMÁTICO"}else{"● SOB DEMANDA"}).color(BLUE));if self.worker.is_some(){ui.spinner();}});
            ui.add_space(12.);ui.label(RichText::new(&self.local).strong());ui.label(format!("Pasta remota: {}",self.remote_name));
            ui.add_space(12.);ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(self.worker.is_none()&&self.paths.configured()&&!self.dirty(),primary_button("Sincronizar agora")).clicked(){match self.paths.set_paused(false){Ok(())=>self.sync(false),Err(e)=>self.message=format!("{e:#}")};}
                if ui.add_enabled(self.worker.is_none()&&self.paths.configured()&&!self.dirty(),egui::Button::new("Ver prévia")).clicked(){self.sync(true);}
                if ui.button("Abrir pasta local").clicked(){match Command::new("xdg-open").arg(&self.local).stdout(Stdio::null()).stderr(Stdio::null()).spawn(){Ok(_)=>{},Err(e)=>self.message=e.to_string()}}
            });
            let previous=self.auto;ui.add_enabled_ui(!self.dirty(), |ui| {ui.checkbox(&mut self.auto,"Sincronizar automaticamente a cada 30 segundos enquanto esta janela estiver aberta");});
            if previous!=self.auto {if let Err(e)=self.paths.set_paused(!self.auto){self.message=e.to_string();self.auto=false;}self.next_cycle=Instant::now();}
            if self.worker.is_some(){ui.label(RichText::new("Pausa entra em vigor após o arquivo atual; não interrompa uma gravação.").small().color(MUTED));}
        });
        ui.add_space(16.);
        let mut resolve = None;
        if let Some(report) = &self.report {
            ui.horizontal(|ui| {
                metric(
                    ui,
                    if self.preview {
                        "Planejadas"
                    } else {
                        "Concluídas"
                    },
                    if self.preview {
                        report.planned.len()
                    } else {
                        report.completed + report.recovered
                    },
                );
                metric(ui, "Conflitos", report.conflicts.len());
                metric(ui, "Bloqueios", report.blocked.len());
            });
            ui.add_space(12.);
            egui::ScrollArea::vertical()
                .max_height(230.)
                .show(ui, |ui| {
                    for conflict in &report.conflicts {
                        if ui
                            .add_enabled(
                                self.worker.is_none(),
                                egui::Button::new(format!("Preservar ambas: {conflict}")),
                            )
                            .clicked()
                        {
                            resolve = Some(conflict.clone());
                        }
                        ui.colored_label(
                            Color32::from_rgb(255, 201, 112),
                            format!("Conflito preservado: {conflict}"),
                        );
                    }
                    for blocked in &report.blocked {
                        ui.colored_label(Color32::from_rgb(255, 165, 165), blocked);
                    }
                    for item in &report.planned {
                        ui.label(item);
                    }
                });
        }
        if let Some(path) = resolve {
            self.spawn(move |paths| {
                paths.keep_both(&path)?;
                Ok(Event::Message(
                    "Versões preservadas. Sincronize para enviar a cópia local.".into(),
                ))
            });
        }
        ui.add_space(12.);
        ui.label(RichText::new("Recuperação: arquivos locais substituídos ficam em .gdrive-clone-recovery; exclusões remotas usam a lixeira do Drive.").small().color(MUTED));
    }
    fn start_login(&mut self) {
        self.account_info = None;
        self.auto = false;
        self.folders_loaded = false;
        self.folders.clear();
        let credentials = PathBuf::from(&self.oauth);
        self.spawn(move |paths| {
            paths.connect(&credentials)?;
            Ok(Event::Connected)
        });
    }
    fn save_setup(&mut self) {
        match self.paths.configure_named(
            Path::new(&self.local),
            &self.remote,
            Some(&self.remote_name),
            Path::new(&self.oauth),
            self.deletions,
        ) {
            Ok(config) => {
                self.local = config.local.display().to_string();
                self.oauth = config.credentials.display().to_string();
                self.saved = Some(self.setup());
                self.report = None;
                self.message = "Preferências salvas. Confira a prévia antes de sincronizar.".into();
            }
            Err(error) => self.message = format!("{error:#}"),
        }
        self.auto = false;
    }
    fn discard_setup(&mut self) {
        if let Some(saved) = self.saved.clone() {
            self.local = saved.local;
            self.remote = saved.remote;
            self.oauth = saved.oauth;
            self.deletions = saved.deletions;
            self.remote_name = self
                .paths
                .load()
                .ok()
                .and_then(|c| c.remote_name)
                .unwrap_or_else(|| "Pasta configurada".into());
        } else {
            self.local.clear();
            self.remote.clear();
            self.remote_name.clear();
            self.deletions = false;
        }
        self.auto = false;
        self.message = "Alterações nas preferências descartadas.".into();
    }
    fn drive_preferences(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().fill(SURFACE).stroke(egui::Stroke::new(1_f32, BORDER))
            .corner_radius(8.).inner_margin(20.).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.heading("Google Drive");
                ui.label("Acesse seus arquivos do Drive neste computador.");
                ui.add_space(4.);
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.local.is_empty() && !self.dirty(), egui::Button::new(RichText::new("Abrir pasta local").color(BLUE))).clicked() {
                        match Command::new("xdg-open").arg(&self.local).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
                            Ok(_) => {}, Err(error) => self.message = error.to_string(),
                        }
                    }
                    ui.hyperlink_to("Saiba mais", "https://support.google.com/drive/answer/13401938?hl=pt-BR");
                });
                ui.add_space(10.);
                ui.separator();
                ui.add_space(12.);
                ui.label(RichText::new("Opções de sincronização de Meu Drive").strong());
                ui.add_space(8.);
                egui::Frame::new().fill(Color32::from_rgb(232, 240, 254))
                    .corner_radius(6.).inner_margin(12.).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            info_symbol(ui);
                            ui.label("O espelhamento mantém cópias no computador e usa espaço no disco local.");
                        });
                    });
                ui.add_space(12.);
                ui.add_enabled(false, egui::RadioButton::new(false, "Transferência de arquivos por streaming"))
                    .on_disabled_hover_text("Streaming ainda não foi implementado nesta versão para Linux.");
                ui.indent("streaming-description", |ui| {
                    ui.label(RichText::new("Arquivos sob demanda, com armazenamento principal na nuvem.").color(MUTED));
                    ui.label(RichText::new("Ainda não disponível nesta versão.").small().color(MUTED));
                });
                ui.add_space(10.);
                ui.scope(|ui| {
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = BLUE;
                    ui.visuals_mut().widgets.inactive.bg_stroke.color = BLUE;
                    ui.add(egui::RadioButton::new(true, RichText::new("Espelhamento de arquivos").strong()));
                });
                ui.indent("mirror-description", |ui| {
                    ui.label("Mantenha seus arquivos no Drive e numa pasta deste computador.");
                    ui.label("Os arquivos baixados ficam disponíveis mesmo sem conexão.");
                });
                ui.add_space(16.);
                ui.separator();
                ui.add_space(10.);
                ui.label(RichText::new("Local dos arquivos espelhados").strong());
                ui.horizontal_wrapped(|ui| {
                    ui.label(if self.local.is_empty() { "Nenhuma pasta local selecionada" } else { &self.local });
                    if ui.button(RichText::new("Alterar…").color(BLUE)).clicked() {
                        self.auto = false;
                        self.spawn(|_| Ok(Event::Picked(Target::Local, dialog::choose(Target::Local)?)));
                    }
                });
                ui.add_space(8.);
                ui.label(RichText::new("Pasta espelhada do Google Drive").strong());
                ui.horizontal_wrapped(|ui| {
                    ui.label(if self.remote.is_empty() { "Nenhuma pasta selecionada" } else { &self.remote_name });
                    if ui.add_enabled(self.paths.account().is_ok(), egui::Button::new(RichText::new("Escolher pasta…").color(BLUE))).clicked() {
                        self.show_folders = true;
                        self.browse(vec![("root".into(), "Meu Drive".into())]);
                    }
                });
            });
    }
    fn computer_preferences(&mut self, ui: &mut egui::Ui) {
        ui.heading("Meu computador");
        ui.label(RichText::new("Pastas deste computador").color(MUTED));
        ui.add_space(34.);
        ui.vertical_centered(|ui| {
            device_illustration(ui);
            ui.add_space(20.);
            ui.label(RichText::new("Seus arquivos locais no Drive").size(20.));
            ui.add_space(8.);
            ui.label("Este aplicativo ainda não registra o dispositivo na seção Computadores do Drive.");
            ui.label("Você pode espelhar seus arquivos em uma pasta de Meu Drive.");
            ui.hyperlink_to("Ver Computadores no navegador", "https://drive.google.com/drive/computers");
            ui.add_space(18.);
            ui.add_enabled(false, primary_button("Adicionar pasta"))
                .on_disabled_hover_text("Registro em Computadores indisponível; não foi identificado suporte documentado na API pública.");
            ui.add_space(20.);
            if ui
                .button(RichText::new("Configurar espelhamento do Google Drive").color(BLUE))
                .clicked()
            {
                self.tab = Tab::Drive;
            }
        });
    }
    fn advanced_preferences(&mut self, ui: &mut egui::Ui) {
        ui.heading("Configurações");
        ui.label(RichText::new("Conta e preferências do aplicativo").color(MUTED));
        ui.add_space(20.);
        ui.label(RichText::new("Conta Google").strong());
        if let Some(info) = &self.account_info {
            if let Some(name) = &info.display_name {
                ui.label(name);
            }
            if let Some(email) = &info.email {
                ui.label(email);
            }
        }
        if ui
            .add_enabled(
                self.paths.account().is_ok(),
                egui::Button::new("Atualizar dados da conta"),
            )
            .clicked()
        {
            self.spawn(|paths| Ok(Event::AccountInfo(paths.account_info()?)));
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.oauth.is_empty(),
                    primary_button("Fazer login com o navegador"),
                )
                .clicked()
            {
                self.start_login();
            }
            if ui
                .add_enabled(
                    self.paths.account().is_ok(),
                    egui::Button::new("Desconectar conta"),
                )
                .clicked()
            {
                self.auto = false;
                self.account_info = None;
                self.folders_loaded = false;
                self.folders.clear();
                self.spawn(|paths| {
                    if paths.configured() {
                        paths.set_paused(true)?;
                    }
                    paths.disconnect()?;
                    Ok(Event::Message(
                        "Credencial local removida; arquivos preservados.".into(),
                    ))
                });
            }
        });
        ui.add_space(16.);
        ui.separator();
        ui.add_space(12.);
        ui.label(RichText::new("Exclusões e recuperação").strong());
        ui.checkbox(
            &mut self.deletions,
            "Propagar exclusões para a lixeira do Drive e a recuperação local",
        );
        if ui
            .add_enabled(
                self.paths.configured() && !self.dirty(),
                egui::Button::new("Recuperar pendências"),
            )
            .clicked()
        {
            self.auto = false;
            self.spawn(|paths| {
                Ok(Event::Message(format!(
                    "{} operações inspecionadas. Execute uma prévia.",
                    paths.replan_pending()?
                )))
            });
        }
        ui.add_space(20.);
        ui.collapsing("Desenvolvimento e configuração manual", |ui| {
            ui.label("Cliente OAuth Desktop fornecido pelo responsável pela compilação");
            ui.text_edit_singleline(&mut self.oauth);
            if ui.button("Escolher arquivo…").clicked() {
                self.auto = false;
                self.spawn(|_| {
                    Ok(Event::Picked(
                        Target::Credentials,
                        dialog::choose(Target::Credentials)?,
                    ))
                });
            }
            ui.label("Pasta local (caminho absoluto)");
            ui.text_edit_singleline(&mut self.local);
            ui.label("ID remoto (root para Meu Drive)");
            if ui.text_edit_singleline(&mut self.remote).changed() {
                self.remote_name = if self.remote == "root" {
                    "Meu Drive"
                } else {
                    "Pasta por ID"
                }
                .into();
            }
        });
        ui.add_space(24.);
        ui.label(
            RichText::new("Drive para Linux · cliente independente")
                .small()
                .color(MUTED),
        );
        ui.label(
            RichText::new("Não é um aplicativo oficial do Google.")
                .small()
                .color(MUTED),
        );
    }
    fn folder_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_folders {
            return;
        }
        let mut open = true;
        let mut chosen = false;
        egui::Window::new("Escolher pasta do Google Drive")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(490.)
            .show(ctx, |ui| {
                let mut navigation = None;
                ui.add_enabled_ui(self.worker.is_none(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (index, (_, name)) in self.folder_trail.iter().enumerate() {
                            if ui.button(name).clicked() {
                                navigation = Some(self.folder_trail[..=index].to_vec());
                            }
                            if index + 1 < self.folder_trail.len() {
                                ui.label("/");
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(260.)
                        .show(ui, |ui| {
                            ui.set_min_height(120.);
                            if self.folders_loaded && self.folders.is_empty() {
                                ui.label("Esta pasta não contém subpastas.");
                            }
                            let mut sibling_name_counts = std::collections::BTreeMap::new();
                            for (_, name) in &self.folders {
                                *sibling_name_counts.entry(name.as_str()).or_insert(0usize) += 1;
                            }
                            for (id, name) in &self.folders {
                                let label = folder_label(
                                    name,
                                    id,
                                    sibling_name_counts.get(name.as_str()).copied().unwrap_or(1),
                                );
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 34.],
                                        egui::Button::new(format!("▸  {label}")),
                                    )
                                    .on_hover_text(format!("Identificador: {id}"))
                                    .clicked()
                                {
                                    let mut trail = self.folder_trail.clone();
                                    trail.push((id.clone(), name.clone()));
                                    navigation = Some(trail);
                                }
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.folders_loaded, primary_button("Usar esta pasta"))
                            .clicked()
                            && let Some((id, name)) = self.folder_trail.last()
                        {
                            self.remote = id.clone();
                            self.remote_name = name.clone();
                            self.auto = false;
                            chosen = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            chosen = true;
                        }
                    });
                });
                if self.worker.is_some() {
                    ui.spinner();
                    ui.label("Processando…");
                }
                if let Some(trail) = navigation {
                    self.browse(trail);
                }
            });
        self.show_folders = open && !chosen;
    }
}

fn folder_label(name: &str, id: &str, sibling_name_count: usize) -> String {
    if sibling_name_count > 1 {
        format!("{name} · ID {id}")
    } else {
        name.to_owned()
    }
}
fn metric(ui: &mut egui::Ui, label: &str, count: usize) {
    egui::Frame::group(ui.style())
        .inner_margin(14.)
        .show(ui, |ui| {
            ui.set_min_width(140.);
            ui.label(RichText::new(count.to_string()).size(28.).color(BLUE));
            ui.label(label);
        });
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames = self.frames.saturating_add(1);
        if let Some(path) = &self.screenshot {
            let screenshot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                match image::save_buffer(
                    path,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {}
                    Err(e) => eprintln!("Falha ao salvar captura: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.frames > 4 && !self.screenshot_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.screenshot_requested = true;
            }
        }
        self.poll();
        if self.auto && !self.dirty() && self.worker.is_none() && Instant::now() >= self.next_cycle
        {
            self.sync(false);
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        self.render_preferences(ctx);
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let (unit, divisor) = if bytes < 1024 * 1024 {
        ("KiB", 1024_u64)
    } else if bytes < 1024 * 1024 * 1024 {
        ("MiB", 1024_u64 * 1024)
    } else {
        ("GiB", 1024_u64 * 1024 * 1024)
    };
    format!("{:.1} {unit}", bytes as f64 / divisor as f64)
}
enum HeaderIcon {
    Help,
    Settings,
    Account,
}
fn header_button(ui: &mut egui::Ui, icon: HeaderIcon, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.), egui::Sense::click());
    let center = rect.center();
    let painter = ui.painter();
    if response.hovered() || response.has_focus() {
        painter.circle_filled(center, 16., SURFACE);
    }
    let stroke = egui::Stroke::new(1.6_f32, MUTED);
    match icon {
        HeaderIcon::Help => {
            painter.circle_stroke(center, 9., stroke);
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(15.),
                MUTED,
            );
        }
        HeaderIcon::Settings => {
            painter.circle_stroke(center, 7., stroke);
            painter.circle_stroke(center, 2.5, stroke);
            for tooth in 0..8 {
                let angle = tooth as f32 * std::f32::consts::TAU / 8.;
                let direction = Vec2::angled(angle);
                painter.line_segment([center + direction * 8., center + direction * 11.], stroke);
            }
        }
        HeaderIcon::Account => {
            painter.circle_filled(center, 15., Color32::from_rgb(211, 227, 253));
            painter.circle_filled(center - Vec2::new(0., 5.), 4., BLUE);
            painter.rect_filled(
                egui::Rect::from_center_size(center + Vec2::new(0., 6.), Vec2::new(15., 8.)),
                4.,
                BLUE,
            );
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}
fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(Color32::WHITE))
        .fill(BLUE)
        .corner_radius(4.)
}
fn navigation_item(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width() - 12., 60.),
        egui::Sense::click(),
    );
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius {
                nw: 0,
                ne: 28,
                sw: 0,
                se: 28,
            },
            if selected {
                Color32::from_rgb(211, 227, 253)
            } else {
                SURFACE
            },
        );
    }
    let color = if selected {
        BLUE
    } else {
        Color32::from_rgb(60, 64, 67)
    };
    ui.painter().text(
        rect.min + Vec2::new(20., 16.),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(15.),
        color,
    );
    ui.painter().text(
        rect.min + Vec2::new(20., 37.),
        egui::Align2::LEFT_CENTER,
        subtitle,
        egui::FontId::proportional(12.),
        MUTED,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            title,
        )
    });
    response
}
// Original geometric drawings; no official brand or proprietary image assets.
fn info_symbol(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.), egui::Sense::hover());
    ui.painter()
        .circle_stroke(rect.center(), 7., egui::Stroke::new(1.5_f32, BLUE));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::proportional(13.),
        BLUE,
    );
}
fn app_symbol(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.), egui::Sense::hover());
    let outer = rect.shrink(3.);
    ui.painter().rect_filled(outer, 7., BLUE);
    ui.painter().rect_stroke(
        outer.shrink(7.),
        2.,
        egui::Stroke::new(1.5_f32, Color32::WHITE),
        egui::StrokeKind::Inside,
    );
    ui.painter().line_segment(
        [
            outer.center_top() + Vec2::new(0., 7.),
            outer.center_bottom() - Vec2::new(0., 7.),
        ],
        egui::Stroke::new(1.5_f32, Color32::WHITE),
    );
}
fn device_illustration(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(200., 120.), egui::Sense::hover());
    let screen = egui::Rect::from_min_size(rect.min + Vec2::new(25., 8.), Vec2::new(150., 90.));
    ui.painter()
        .rect_filled(screen, 8., Color32::from_rgb(232, 240, 254));
    ui.painter().rect_stroke(
        screen,
        8.,
        egui::Stroke::new(2_f32, BLUE),
        egui::StrokeKind::Inside,
    );
    ui.painter().line_segment(
        [
            rect.min + Vec2::new(10., 105.),
            rect.min + Vec2::new(190., 105.),
        ],
        egui::Stroke::new(5_f32, BLUE),
    );
    let folder = egui::Rect::from_min_size(screen.min + Vec2::new(48., 30.), Vec2::new(54., 38.));
    ui.painter().rect_filled(folder, 4., BLUE);
    ui.painter().rect_filled(
        egui::Rect::from_min_size(folder.min - Vec2::new(0., 7.), Vec2::new(23., 12.)),
        3.,
        BLUE,
    );
}

#[cfg(test)]
mod tests;
