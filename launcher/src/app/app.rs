use std::collections::HashSet;

use eframe::egui;
use eframe::run_native;
use tokio::runtime::Runtime;

use super::auth_state::AuthState;
use super::instance_sync_state::InstanceSyncState;
use super::java_state::JavaState;
use super::launch_state::ForceLaunchResult;
use super::launch_state::LaunchState;
use super::manifest_state::ManifestState;
use super::metadata_state::MetadataState;
use super::new_instance_state::NewInstanceState;
use super::settings::SettingsState;
use crate::config::build_config;
use crate::config::runtime_config::Config;
use crate::utils;
use crate::version::instance_storage::InstanceStatus;
use crate::version::instance_storage::InstanceStorage;
use crate::version::instance_storage::LocalInstance;

pub struct LauncherApp {
    runtime: Runtime,

    config: Config,
    instance_storage: InstanceStorage,

    settings_state: SettingsState,
    auth_state: AuthState,
    manifest_state: ManifestState,
    metadata_state: MetadataState,
    java_state: JavaState,
    instance_sync_state: InstanceSyncState,
    launch_state: LaunchState,
    new_instance_state: NewInstanceState,
}

pub fn run_gui(config: Config, launch: bool) {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size((670.0, 450.0))
            .with_icon(utils::get_icon_data()),
        ..Default::default()
    };

    run_native(
        &build_config::get_launcher_name(),
        native_options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(LauncherApp::new(config, &cc.egui_ctx, launch)))
        }),
    )
    .unwrap();
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill.to_normalized_gamma_f32()
    }
}

impl LauncherApp {
    fn new(config: Config, ctx: &egui::Context, launch: bool) -> Self {
        let runtime = Runtime::new().unwrap();

        LauncherApp {
            settings_state: SettingsState::new(),
            auth_state: AuthState::new(ctx, &config),
            manifest_state: ManifestState::new(&runtime, ctx),
            metadata_state: MetadataState::new(),
            java_state: JavaState::new(ctx),
            instance_sync_state: InstanceSyncState::new(ctx),
            launch_state: LaunchState::new(launch),
            new_instance_state: NewInstanceState::new(&runtime, ctx),
            instance_storage: runtime.block_on(InstanceStorage::load(&config)),
            config,
            runtime,
        }
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    let selected_metadata = self.metadata_state.get_version_metadata(&self.config);
                    let selected_metadata_ref = selected_metadata.as_deref();
                    self.settings_state.render_ui(
                        ui,
                        &self.runtime,
                        &mut self.config,
                        selected_metadata_ref,
                    );

                    self.instance_sync_state.render_sync_button(
                        ui,
                        &self.runtime,
                        &mut self.config,
                        selected_metadata,
                    );

                    if ui.button("🔄").clicked() {
                        self.auth_state.reset(&mut self.config, &self.runtime, ctx);
                        self.manifest_state.retry_fetch(&self.runtime, ctx);
                        self.metadata_state.reset(); // just reset the state, not the task

                        // metadata is checked after manifest is fetched
                        // java is checked after metadata is fetched
                    }
                });
                ui.add_space(5.0);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(&ctx.style())
                    .outer_margin(egui::Margin::symmetric(150.0, 100.0))
                    .inner_margin(egui::Margin::same(30.0))
                    .stroke(ctx.style().visuals.widgets.noninteractive.bg_stroke)
                    .rounding(egui::Rounding::same(10.0)),
            )
            .show(ctx, |ui| {
                self.render_central_elements(ui, ctx);
            });
    }

    fn get_selected_instance(&self, config: &Config) -> Option<LocalInstance> {
        self.instance_storage
            .get_instance(config.selected_instance_name.as_ref()?)
    }

    fn set_metadata_task(&mut self, ctx: &egui::Context) {
        if let Some(selected_instance) = self.get_selected_instance(&self.config) {
            self.metadata_state.set_metadata_task(
                &self.runtime,
                &self.config,
                &selected_instance.version_info,
                ctx,
            );
        }
    }

    fn render_central_elements(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (manifest, updated) = self.manifest_state.take_manifest(&mut self.config);
        if let Some(manifest) = manifest {
            self.instance_storage.set_remote_manifest(Some(manifest));
        }
        if updated {
            self.set_metadata_task(ctx);
        }

        if let Some(version_info) = self.new_instance_state.take_new_instance() {
            self.runtime.block_on(
                self.instance_storage
                    .add_instance(&mut self.config, version_info),
            );
        }

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                let (local_instance_names, remote_instance_names) =
                    self.instance_storage.get_all_names();

                let mut all_names: HashSet<String> =
                    local_instance_names.clone().into_iter().collect();
                all_names.extend(remote_instance_names.clone());
                let new_instance_result = self.new_instance_state.render_ui(
                    &self.runtime,
                    ui,
                    &mut self.config,
                    &all_names,
                    &local_instance_names,
                );

                if let Some(instance_to_delete) = new_instance_result.instance_to_delete {
                    self.config.auth_profiles.remove(&instance_to_delete);
                    self.config.save();
                    self.runtime.block_on(
                        self.instance_storage
                            .delete_instance(&mut self.config, &instance_to_delete),
                    );
                    self.instance_sync_state.reset_status();
                }

                let selected_version_changed = self.manifest_state.render_combo_box(
                    ui,
                    &mut self.config,
                    &local_instance_names,
                    &remote_instance_names,
                );
                if selected_version_changed {
                    self.instance_sync_state.cancel_sync();
                    self.set_metadata_task(ctx);
                }
            });
        });

        self.auth_state.update(&self.runtime, &mut self.config);

        ui.vertical_centered(|ui| {
            self.metadata_state.render_ui(ui, &self.config);
            let selected_instance = self.metadata_state.get_version_metadata(&self.config);
            self.instance_sync_state.render_ui(
                ui,
                &self.runtime,
                &mut self.config,
                selected_instance,
            );
        });

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                let version_metadata = self.metadata_state.get_version_metadata(&self.config);
                let auth_backend =
                    version_metadata.and_then(|metadata| metadata.get_auth_backend().cloned());
                self.auth_state.render_ui(
                    ui,
                    &mut self.config,
                    &self.runtime,
                    ctx,
                    auth_backend.as_ref(),
                );
            });
        });

        if let Some(selected_instance) = self.get_selected_instance(&self.config) {
            if self.metadata_state.update() {
                if self.manifest_state.online()
                    && self.metadata_state.online()
                    && selected_instance.status == InstanceStatus::UpToDate
                {
                    self.instance_sync_state.set_up_to_date();
                } else {
                    self.instance_sync_state.reset_status();
                }

                if let Some(version_metadata) =
                    self.metadata_state.get_version_metadata(&self.config)
                {
                    self.java_state.set_check_java_task(
                        &self.runtime,
                        &version_metadata,
                        &mut self.config,
                        ctx,
                    );
                }
            }

            if let Some(version_metadata) = self.metadata_state.get_version_metadata(&self.config) {
                if self.instance_sync_state.update() {
                    self.runtime.block_on(
                        self.instance_storage
                            .mark_downloaded(&mut self.config, version_metadata.get_name()),
                    );
                }

                self.java_state
                    .update(&self.runtime, &version_metadata, &mut self.config, ctx);
            }
        }

        ui.vertical_centered(|ui| {
            let selected_instance = self.metadata_state.get_version_metadata(&self.config);
            self.java_state
                .render_ui(ui, &mut self.config, selected_instance.as_deref());

            self.launch_state.update();

            if self.java_state.ready_for_launch()
                && self
                    .get_selected_instance(&self.config)
                    .map_or(false, |instance| {
                        instance.status == InstanceStatus::UpToDate
                    })
            {
                let auth_data = self.auth_state.get_auth_data(&self.config);
                let selected_instance = self.metadata_state.get_version_metadata(&self.config);
                self.launch_state.render_ui(
                    &self.runtime,
                    ui,
                    &mut self.config,
                    selected_instance,
                    auth_data,
                    !self.auth_state.offline()
                        && self.manifest_state.online()
                        && self.metadata_state.online(),
                    self.instance_sync_state.is_syncing()
                        || self.manifest_state.is_fetching()
                        || self.metadata_state.is_getting(),
                );
            } else {
                let some_version_selected = self.get_selected_instance(&self.config).is_some();
                let have_some_auth_data = self.auth_state.get_auth_data(&self.config).is_some();
                let force_launch_result = self.launch_state.render_download_ui(
                    ui,
                    &mut self.config,
                    self.instance_sync_state.is_syncing()
                        || self.manifest_state.is_fetching()
                        || self.metadata_state.is_getting()
                        || self.java_state.checking_java()
                        || !some_version_selected
                        || !have_some_auth_data,
                );
                match force_launch_result {
                    ForceLaunchResult::ForceLaunchSelected => {
                        if let Some(version_metadata) =
                            self.metadata_state.get_version_metadata(&self.config)
                        {
                            self.instance_sync_state.schedule_sync_if_needed(
                                &self.runtime,
                                version_metadata.clone(),
                                false,
                                &mut self.config,
                                ctx,
                            );
                            self.java_state.schedule_download_if_needed(
                                &self.runtime,
                                &version_metadata,
                                &mut self.config,
                            );
                        }
                    }
                    ForceLaunchResult::CancelSelected => {
                        self.java_state.cancel_download();
                        self.instance_sync_state.cancel_sync();
                    }
                    ForceLaunchResult::NotSelected => {}
                }
            }
        });
    }
}
