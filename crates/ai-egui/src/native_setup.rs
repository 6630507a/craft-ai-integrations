//! A neutral setup surface. Hosts keep their own chrome and use the same SetupSession state.
use craft_ai_client::{CheckedNative, SetupRequest, SetupSession, SetupTicket};
use egui::Ui;

/// What the host should do after a frame. `Work` runs off the UI thread; the others are plain host actions.
#[derive(Clone, Debug)]
pub enum NativeSetupAction {
    Work(SetupRequest),
    Use(CheckedNative),
    FindFiles,
    ChangeFolder,
    Cancel(SetupTicket),
    Back,
}

/// Label of the licence tick box.
pub const ACCEPT_LICENCES: &str = "I accept the model's licences";
/// Label of the download button.
pub const DOWNLOAD_MODEL: &str = "Download model files";
/// Label of the button that checks files already on disk.
pub const CHECK_FILES: &str = "Check existing files";
/// Label of the button that applies checked files; the host makes the choice, not the widget.
pub const USE_MODEL: &str = "Use this model";

/// Draw only. `Work` is run off-thread; the host reports its answer with `session.complete`.
pub fn native_model_setup(ui: &mut Ui, session: &mut SetupSession) -> Option<NativeSetupAction> {
    let plan = session.plan().clone();
    ui.heading(format!("Set up {}", plan.feature().title));
    ui.label(&plan.feature().summary);
    ui.label("The model choice is shared with every app that supports this tool.");
    if let Some(checked) = session.checked() {
        ui.label("Files checked. This model has not been tried yet.");
        ui.label(checked.weights().to_string_lossy());
        if ui.button(USE_MODEL).clicked() {
            return Some(NativeSetupAction::Use(checked.clone()));
        }
        if ui.button("Back").clicked() {
            return Some(NativeSetupAction::Back);
        }
        return None;
    }
    ui.label(format!(
        "Download {}",
        crate::text::bytes(plan.download_bytes())
    ));
    ui.label("Save model files to:");
    if let Some(destination) = plan.destination() {
        ui.label(destination.to_string_lossy());
    } else {
        ui.label("Choose a model folder before downloading.");
    }
    let mut licences = Vec::new();
    for file in &plan.route().models {
        if !licences.contains(&file.licence) {
            licences.push(file.licence.clone());
        }
    }
    for licence in licences {
        if let Some(url) = licence
            .url
            .as_deref()
            .filter(|url| url.starts_with("https://"))
        {
            ui.hyperlink_to(format!("Read {}", licence.name), url);
        } else {
            ui.label(&licence.name);
        }
        if let Some(note) = &licence.note {
            ui.label(note);
        }
    }
    if session.busy() {
        if let Some((file, progress)) = session.progress() {
            let verb = match progress.phase {
                craft_ai_client::SetupPhase::Downloading => "Downloading",
                craft_ai_client::SetupPhase::Verifying => "Checking",
            };
            ui.label(format!(
                "{verb} file {} of {}",
                file + 1,
                plan.route().models.len()
            ));
            let fraction = if progress.total == 0 {
                0.0
            } else {
                (progress.done as f64 / progress.total as f64).clamp(0.0, 1.0) as f32
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_width(220.0)
                    .show_percentage(),
            );
        } else {
            ui.label("Working on model files.");
        }
        if ui.button("Cancel").clicked() {
            return session.cancel().map(NativeSetupAction::Cancel);
        }
        return None;
    }
    if let Some(kind) = session.problem() {
        ui.colored_label(ui.visuals().warn_fg_color, setup_problem(kind));
    }
    if !plan.available() {
        ui.label("This app can't use this option yet. Choose another option.");
    }
    if let Some(directory) = plan.existing_directory() {
        ui.label("Existing model files:");
        ui.label(directory.to_string_lossy());
        if ui
            .add_enabled(plan.available(), egui::Button::new(CHECK_FILES))
            .clicked()
        {
            return session
                .begin(craft_ai_client::SetupTask::CheckExisting)
                .map(NativeSetupAction::Work);
        }
    }
    let mut accepted = session.accepted();
    ui.add_enabled_ui(plan.available(), |ui| {
        ui.checkbox(&mut accepted, ACCEPT_LICENCES);
    });
    session.accept_licences(accepted);
    if ui
        .add_enabled(
            plan.available() && accepted && plan.destination().is_some(),
            egui::Button::new(DOWNLOAD_MODEL),
        )
        .clicked()
    {
        return session
            .begin(craft_ai_client::SetupTask::Download)
            .map(NativeSetupAction::Work);
    }
    if ui
        .add_enabled(plan.available(), egui::Button::new("Find model files"))
        .clicked()
    {
        return Some(NativeSetupAction::FindFiles);
    }
    if ui
        .add_enabled(plan.available(), egui::Button::new("Change folder"))
        .clicked()
    {
        return Some(NativeSetupAction::ChangeFolder);
    }
    if ui.button("Back").clicked() {
        return Some(NativeSetupAction::Back);
    }
    None
}

/// Safe wording for host renderers too. A source's error body is intentionally not displayed.
pub fn setup_problem(kind: craft_ai_core::ErrorKind) -> &'static str {
    use craft_ai_core::ErrorKind;
    match kind {
        ErrorKind::Auth => {
            "Access to these model files was refused. Check access with their maker."
        }
        ErrorKind::Cancelled => "Setup was cancelled. Partial downloads can be resumed.",
        ErrorKind::Network | ErrorKind::RateLimited => {
            "The download could not finish. Check your connection and try again."
        }
        ErrorKind::InvalidRequest => {
            "The model files could not be checked. Review the files and chosen folder."
        }
        ErrorKind::Unsupported => "This app can't use this option yet. Choose another option.",
        _ => {
            "The model files could not be prepared. Nothing has been switched on. You can try again."
        }
    }
}
