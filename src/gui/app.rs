use super::schema;
use super::validation::{self, ValidationIssue};
use crate::config::{self, Config, ConfigSource, LoadedConfig};
use anyhow::Context;
use eframe::egui;
use eframe::egui::widgets::color_picker::{self, Alpha};
use std::path::{Path, PathBuf};

pub struct ConfigEditorApp {
    config: Config,
    last_saved_config: Config,
    current_path: PathBuf,
    selected_section: schema::Section,
    validation_issues: Vec<ValidationIssue>,
    status: Option<StatusMessage>,
    modal: Option<ModalState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
struct StatusMessage {
    kind: StatusKind,
    text: String,
}

#[derive(Debug, Clone)]
enum ModalState {
    OpenPath { input: String },
    SaveAs { input: String },
    ReloadConfirm,
}

impl ConfigEditorApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, initial: LoadedConfig) -> Self {
        let status = match (&initial.source, &initial.load_error) {
            (ConfigSource::Disk, _) => Some(StatusMessage {
                kind: StatusKind::Info,
                text: format!("Loaded {}", initial.path.display()),
            }),
            (ConfigSource::Defaults, Some(error)) => Some(StatusMessage {
                kind: StatusKind::Warning,
                text: format!("{error}. Editing defaults for {}", initial.path.display()),
            }),
            (ConfigSource::Defaults, None) => Some(StatusMessage {
                kind: StatusKind::Info,
                text: format!(
                    "No config file found. Editing defaults for {}",
                    initial.path.display()
                ),
            }),
        };

        let validation_issues = validation::validate_config(&initial.config);

        Self {
            config: initial.config.clone(),
            last_saved_config: initial.config,
            current_path: initial.path,
            selected_section: schema::Section::Hint,
            validation_issues,
            status,
            modal: None,
        }
    }

    fn draw_root(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            self.toolbar(ui);
        });

        egui::Panel::left("sections")
            .resizable(false)
            .default_size(220.0)
            .show_inside(ui, |ui| {
                self.section_list(ui);
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let changed = self.section_content(ui);
            if changed {
                self.persist_if_valid();
            }
        });

        self.show_modal(&ctx);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("warpd-rs configuration");
            ui.separator();
            ui.label(format!("Path: {}", self.current_path.display()));

            if self.has_unsaved_changes() {
                ui.separator();
                ui.colored_label(ui.visuals().warn_fg_color, "Unsaved changes");
            }
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Open path").clicked() {
                self.modal = Some(ModalState::OpenPath {
                    input: self.current_path.display().to_string(),
                });
            }

            if ui.button("Save as").clicked() {
                self.modal = Some(ModalState::SaveAs {
                    input: self.current_path.display().to_string(),
                });
            }

            if ui.button("Reload from disk").clicked() {
                self.modal = Some(ModalState::ReloadConfirm);
            }

            if ui.button("Reset to defaults").clicked() {
                self.config = Config::default();
                self.status = Some(StatusMessage {
                    kind: StatusKind::Info,
                    text: "Reset editor state to defaults. Changes will auto-save once valid."
                        .into(),
                });
                self.persist_if_valid();
            }
        });

        ui.add_space(6.0);
        if let Some(status) = &self.status {
            let color = match status.kind {
                StatusKind::Info => ui.visuals().strong_text_color(),
                StatusKind::Warning => ui.visuals().warn_fg_color,
                StatusKind::Error => ui.visuals().error_fg_color,
            };
            ui.colored_label(color, &status.text);
        }

        if !self.validation_issues.is_empty() {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!(
                        "{} validation issue(s) blocking auto-save",
                        self.validation_issues.len()
                    ),
                );

                for issue in self.validation_issues.iter().take(4) {
                    ui.label(format!("{}: {}", issue.field, issue.message));
                }
            });
        }
    }

    fn section_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sections");
        ui.label("Choose a config group to edit.");
        ui.add_space(12.0);

        for section in schema::Section::ALL {
            let selected = self.selected_section == section;
            if ui.selectable_label(selected, section.title()).clicked() {
                self.selected_section = section;
            }
            if selected {
                ui.small(section.description());
                ui.add_space(8.0);
            }
        }
    }

    fn section_content(&mut self, ui: &mut egui::Ui) -> bool {
        ui.heading(self.selected_section.title());
        ui.label(self.selected_section.description());
        ui.add_space(12.0);

        match self.selected_section {
            schema::Section::Hint => self.draw_hint_section(ui),
            schema::Section::Grid => self.draw_grid_section(ui),
            schema::Section::Normal => self.draw_normal_section(ui),
            schema::Section::Cursor => self.draw_cursor_section(ui),
            schema::Section::Mouse => self.draw_mouse_section(ui),
        }
    }

    fn draw_hint_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        changed |= Self::choice_row(ui, &schema::HINT_SOURCE, &mut self.config.hint_source);
        changed |= Self::text_row(ui, &schema::HINT_CHARS, &mut self.config.hint_chars);
        changed |= Self::color_row(ui, &schema::HINT_BGCOLOR, &mut self.config.hint_bgcolor);
        changed |= Self::slider_f64_row(
            ui,
            &schema::HINT_BG_OPACITY,
            &mut self.config.hint_bg_opacity,
        );
        changed |= Self::color_row(ui, &schema::HINT_FGCOLOR, &mut self.config.hint_fgcolor);
        changed |= Self::slider_f64_row(
            ui,
            &schema::HINT_BORDER_RADIUS,
            &mut self.config.hint_border_radius,
        );
        changed |= Self::text_row(ui, &schema::HINT_FONT, &mut self.config.hint_font);
        changed |=
            Self::slider_f64_row(ui, &schema::HINT_FONT_SIZE, &mut self.config.hint_font_size);
        changed |= Self::slider_u32_row(ui, &schema::HINT_GRID_GAP, &mut self.config.hint_grid_gap);
        changed
    }

    fn draw_grid_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        changed |= Self::color_row(ui, &schema::GRID_COLOR, &mut self.config.grid_color);
        changed |= Self::slider_u32_row(
            ui,
            &schema::GRID_BORDER_SIZE,
            &mut self.config.grid_border_size,
        );
        changed |=
            Self::slider_f64_row(ui, &schema::GRID_FONT_SIZE, &mut self.config.grid_font_size);
        changed |= Self::slider_f64_row(ui, &schema::GRID_MIN_SIZE, &mut self.config.grid_min_size);
        changed |= Self::array_row(
            ui,
            "Grid quadrant keys",
            "Keys used for top-left, top-right, bottom-left, and bottom-right selection.",
            &schema::GRID_KEY_LABELS,
            &mut self.config.grid_quadrant_keys,
        );
        changed
    }

    fn draw_normal_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        changed |= Self::slider_u32_row(ui, &schema::SPEED, &mut self.config.speed);
        changed |= Self::slider_u32_row(ui, &schema::ACCELERATION, &mut self.config.acceleration);
        changed |= Self::array_row(
            ui,
            "Movement keys",
            "Bindings used for left, down, up, and right movement.",
            &schema::MOVE_KEY_LABELS,
            &mut self.config.normal_move_keys,
        );
        changed |= Self::text_row(
            ui,
            &schema::SPEED_MODIFIER_KEY,
            &mut self.config.speed_modifier_key,
        );
        changed |= Self::slider_f64_row(
            ui,
            &schema::SPEED_MODIFIER_MULTIPLIER,
            &mut self.config.speed_modifier_multiplier,
        );
        changed
    }

    fn draw_cursor_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        changed |= Self::color_row(ui, &schema::CURSOR_COLOR, &mut self.config.cursor_color);
        changed |= Self::slider_u32_row(ui, &schema::CURSOR_SIZE, &mut self.config.cursor_size);
        changed |= Self::slider_u32_row(
            ui,
            &schema::CROSSHAIR_LINE_WIDTH,
            &mut self.config.crosshair_line_width,
        );
        changed
    }

    fn draw_mouse_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        changed |= Self::array_row(
            ui,
            "Mouse buttons",
            "Bindings used for left, middle, and right click actions.",
            &schema::BUTTON_LABELS,
            &mut self.config.buttons,
        );
        changed |= Self::bool_row(
            ui,
            &schema::CLICK_AFTER_WARP,
            &mut self.config.click_after_warp,
        );
        changed
    }

    fn choice_row(ui: &mut egui::Ui, spec: &schema::ChoiceSpec, value: &mut String) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(spec.label);
            ui.small(spec.help);

            egui::ComboBox::from_id_salt(spec.label)
                .selected_text(value.as_str())
                .show_ui(ui, |ui| {
                    for option in spec.options {
                        if ui
                            .selectable_value(value, (*option).to_owned(), *option)
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
        });
        ui.add_space(8.0);
        changed
    }

    fn text_row(ui: &mut egui::Ui, spec: &schema::TextSpec, value: &mut String) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(spec.label);
            ui.small(spec.help);
            changed = ui.text_edit_singleline(value).changed();
        });
        ui.add_space(8.0);
        changed
    }

    fn bool_row(ui: &mut egui::Ui, spec: &schema::BoolSpec, value: &mut bool) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            changed = ui.checkbox(value, spec.label).changed();
            ui.small(spec.help);
        });
        ui.add_space(8.0);
        changed
    }

    fn slider_u32_row(ui: &mut egui::Ui, spec: &schema::SliderSpecU32, value: &mut u32) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(spec.label);
            ui.small(spec.help);
            changed = ui
                .add(egui::Slider::new(value, spec.range.clone()))
                .changed();
        });
        ui.add_space(8.0);
        changed
    }

    fn slider_f64_row(ui: &mut egui::Ui, spec: &schema::SliderSpecF64, value: &mut f64) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(spec.label);
            ui.small(spec.help);
            changed = ui
                .add(egui::Slider::new(value, spec.range.clone()))
                .changed();
        });
        ui.add_space(8.0);
        changed
    }

    fn color_row(ui: &mut egui::Ui, spec: &schema::TextSpec, value: &mut String) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(spec.label);
            ui.small(spec.help);
            ui.horizontal(|ui| {
                let mut color = config::color32_from_hex(value)
                    .map(|[r, g, b, a]| egui::Color32::from_rgba_unmultiplied(r, g, b, a))
                    .unwrap_or(egui::Color32::from_rgba_unmultiplied(97, 0, 255, 128));

                if color_picker::color_edit_button_srgba(ui, &mut color, Alpha::OnlyBlend).changed()
                {
                    *value = config::hex_from_rgba8([color.r(), color.g(), color.b(), color.a()]);
                    changed = true;
                }

                if ui.text_edit_singleline(value).changed() {
                    changed = true;
                }
            });
        });
        ui.add_space(8.0);
        changed
    }

    fn array_row<const N: usize>(
        ui: &mut egui::Ui,
        label: &str,
        help: &str,
        item_labels: &[&str; N],
        values: &mut [String; N],
    ) -> bool {
        let mut changed = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(label);
            ui.small(help);

            for (item_label, value) in item_labels.iter().zip(values.iter_mut()) {
                ui.horizontal(|ui| {
                    ui.label(*item_label);
                    if ui.text_edit_singleline(value).changed() {
                        changed = true;
                    }
                });
            }
        });
        ui.add_space(8.0);
        changed
    }

    fn persist_if_valid(&mut self) {
        self.validation_issues = validation::validate_config(&self.config);
        if !self.validation_issues.is_empty() {
            self.status = Some(StatusMessage {
                kind: StatusKind::Error,
                text: "Fix validation issues before changes can be written to disk.".into(),
            });
            return;
        }

        match self.config.save_to_path(&self.current_path) {
            Ok(()) => {
                self.last_saved_config = self.config.clone();
                self.status = Some(StatusMessage {
                    kind: StatusKind::Info,
                    text: format!("Saved {}", self.current_path.display()),
                });
            }
            Err(error) => {
                self.status = Some(StatusMessage {
                    kind: StatusKind::Error,
                    text: format!("Failed to save {}: {error}", self.current_path.display()),
                });
            }
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        self.config != self.last_saved_config
    }

    fn replace_loaded_config(&mut self, loaded: LoadedConfig) {
        self.current_path = loaded.path;
        self.config = loaded.config.clone();
        self.last_saved_config = loaded.config;
        self.validation_issues = validation::validate_config(&self.config);
        self.status = match (loaded.source, loaded.load_error) {
            (ConfigSource::Disk, _) => Some(StatusMessage {
                kind: StatusKind::Info,
                text: format!("Loaded {}", self.current_path.display()),
            }),
            (ConfigSource::Defaults, Some(error)) => Some(StatusMessage {
                kind: StatusKind::Warning,
                text: format!(
                    "{error}. Editing defaults for {}",
                    self.current_path.display()
                ),
            }),
            (ConfigSource::Defaults, None) => Some(StatusMessage {
                kind: StatusKind::Info,
                text: format!(
                    "No config file found. Editing defaults for {}",
                    self.current_path.display()
                ),
            }),
        };
    }

    fn open_path(&mut self, path: &Path) {
        let loaded = Config::load_with_source(Some(path));
        self.replace_loaded_config(loaded);
    }

    fn save_as(&mut self, path: &Path) {
        self.validation_issues = validation::validate_config(&self.config);
        if !self.validation_issues.is_empty() {
            self.status = Some(StatusMessage {
                kind: StatusKind::Error,
                text: "Cannot save because the current config is invalid.".into(),
            });
            return;
        }

        match self
            .config
            .save_to_path(path)
            .with_context(|| format!("failed to save {}", path.display()))
        {
            Ok(()) => {
                self.current_path = path.to_path_buf();
                self.last_saved_config = self.config.clone();
                self.status = Some(StatusMessage {
                    kind: StatusKind::Info,
                    text: format!("Saved {}", self.current_path.display()),
                });
            }
            Err(error) => {
                self.status = Some(StatusMessage {
                    kind: StatusKind::Error,
                    text: error.to_string(),
                });
            }
        }
    }

    fn reload_current_path(&mut self) {
        let loaded = Config::load_with_source(Some(&self.current_path));
        self.replace_loaded_config(loaded);
    }

    fn show_modal(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal.clone() else {
            return;
        };

        match modal {
            ModalState::OpenPath { mut input } => {
                let mut keep_open = true;
                egui::Modal::new(egui::Id::new("open_config_path_modal")).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.heading("Open config path");
                    ui.label("Load config from a TOML path. If parsing fails, defaults will be shown against that path.");
                    ui.text_edit_singleline(&mut input);
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            let path = PathBuf::from(input.trim());
                            self.open_path(&path);
                            keep_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep_open = false;
                        }
                    });
                });

                self.modal = if keep_open {
                    Some(ModalState::OpenPath { input })
                } else {
                    None
                };
            }
            ModalState::SaveAs { mut input } => {
                let mut keep_open = true;
                egui::Modal::new(egui::Id::new("save_as_modal")).show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.heading("Save config as");
                    ui.label(
                        "Write the current config to a new TOML path and switch the editor to it.",
                    );
                    ui.text_edit_singleline(&mut input);
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            let path = PathBuf::from(input.trim());
                            self.save_as(&path);
                            keep_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep_open = false;
                        }
                    });
                });

                self.modal = if keep_open {
                    Some(ModalState::SaveAs { input })
                } else {
                    None
                };
            }
            ModalState::ReloadConfirm => {
                let mut keep_open = true;
                egui::Modal::new(egui::Id::new("reload_modal")).show(ctx, |ui| {
                    ui.set_width(360.0);
                    ui.heading("Reload from disk?");
                    ui.label("This replaces the current editor state with the contents of the active config path.");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Reload").clicked() {
                            self.reload_current_path();
                            keep_open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            keep_open = false;
                        }
                    });
                });

                self.modal = if keep_open {
                    Some(ModalState::ReloadConfirm)
                } else {
                    None
                };
            }
        }
    }
}

impl eframe::App for ConfigEditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.draw_root(ui, frame);
    }
}
