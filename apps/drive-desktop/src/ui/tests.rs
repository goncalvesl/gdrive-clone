#![allow(clippy::unwrap_used)]
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    directory: PathBuf,
    app: App,
}
impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "drive-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let credentials = directory.join("synthetic.json");
        std::fs::write(
            &credentials,
            br#"{"installed":{"client_id":"synthetic","client_secret":"synthetic"}}"#,
        )
        .unwrap();
        let paths = Paths {
            config: directory.join("config"),
            data: directory.join("data"),
        };
        paths
            .configure_named(
                &directory.join("files"),
                "synthetic-folder",
                Some("Arquivos de teste"),
                &credentials,
                false,
            )
            .unwrap();
        Self {
            directory,
            app: App::new(paths, None),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn render(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(940., 720.),
            )),
            events,
            ..Default::default()
        },
        |ctx| app.render_preferences(ctx),
    )
}
fn click(app: &mut App, ctx: &egui::Context, position: egui::Pos2) {
    render(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    render(
        app,
        ctx,
        vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}
#[test]
fn preferences_navigation_preserves_saved_configuration() {
    let mut fixture = Fixture::new();
    let ctx = egui::Context::default();
    let file = fixture.app.paths.config.join("config.json");
    let original = std::fs::read(&file).unwrap();
    render(&mut fixture.app, &ctx, vec![]);
    click(&mut fixture.app, &ctx, egui::pos2(80., 100.));
    assert!(matches!(fixture.app.tab, Tab::Computer));
    click(&mut fixture.app, &ctx, egui::pos2(80., 172.));
    assert!(matches!(fixture.app.tab, Tab::Drive));
    assert!(fixture.app.worker.is_none());
    assert!(!fixture.app.dirty());
    assert_eq!(std::fs::read(file).unwrap(), original);
}
#[test]
fn cancel_discards_draft_without_writing_configuration_or_starting_sync() {
    let mut fixture = Fixture::new();
    let file = fixture.app.paths.config.join("config.json");
    let original = std::fs::read(&file).unwrap();
    let local = fixture.app.local.clone();
    fixture.app.local = fixture.directory.join("unsaved").display().to_string();
    fixture.app.remote = "another-folder".into();
    fixture.app.remote_name = "Outro nome".into();
    fixture.app.deletions = true;
    assert!(fixture.app.dirty());
    fixture.app.discard_setup();
    assert!(!fixture.app.dirty());
    assert_eq!(fixture.app.local, local);
    assert_eq!(fixture.app.remote_name, "Arquivos de teste");
    assert!(!fixture.directory.join("unsaved").exists());
    assert_eq!(std::fs::read(file).unwrap(), original);
    assert!(fixture.app.worker.is_none());
}

#[test]
fn automatic_sync_retries_transient_errors_but_stops_on_permanent_errors() {
    let mut fixture = Fixture::new();
    fixture.app.auto = true;
    let (tx, rx) = mpsc::channel();
    fixture.app.worker = Some(rx);
    tx.send(Err(WorkerFailure {
        message: "synthetic network error".into(),
        retry_minimum: Some(Duration::from_secs(90)),
    }))
    .unwrap();
    let before = Instant::now();
    fixture.app.poll();
    assert!(fixture.app.auto);
    assert_eq!(fixture.app.retry_failures, 1);
    assert!(fixture.app.next_cycle.duration_since(before).as_secs() >= 90);
    let (tx, rx) = mpsc::channel();
    fixture.app.worker = Some(rx);
    tx.send(Err(WorkerFailure {
        message: "synthetic precondition error".into(),
        retry_minimum: None,
    }))
    .unwrap();
    fixture.app.poll();
    assert!(!fixture.app.auto);
    assert!(fixture.app.worker.is_none());
}

#[test]
fn receiving_all_transfer_bytes_does_not_mark_a_sync_as_completed() {
    let mut fixture = Fixture::new();
    let (progress_tx, progress_rx) = mpsc::channel();
    fixture.app.progress_rx = Some(progress_rx);
    let (worker_tx, worker_rx) = mpsc::channel();
    fixture.app.worker = Some(worker_rx);
    progress_tx
        .send(drive_api::TransferProgress {
            path: "file".into(),
            upload: true,
            bytes: 3,
            total: 3,
        })
        .unwrap();
    fixture.app.poll();
    assert!(fixture.app.transfer.is_some());
    assert!(fixture.app.report.is_none());
    assert!(fixture.app.worker.is_some());
    worker_tx
        .send(Ok(Event::Report(
            Report {
                completed: 1,
                ..Default::default()
            },
            false,
        )))
        .unwrap();
    fixture.app.poll();
    assert!(fixture.app.transfer.is_none());
    assert_eq!(fixture.app.report.as_ref().unwrap().completed, 1);
}
