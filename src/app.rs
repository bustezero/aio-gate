use egui::{ColorImage, TextureHandle};
use image::io::Reader as ImageReader;
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use sevenz_rust::{Archive, BlockDecoder, Password};
use std::{fs::File, io::Result, path::PathBuf};
use tempfile::{tempdir, TempDir};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    label: String,
    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    #[serde(skip)]
    selected_file: Option<PathBuf>,
    #[serde(skip)]
    file_list: Vec<String>,
    #[serde(skip)]
    temp_dir: Option<TempDir>,
    #[serde(skip)]
    extracted_file: Option<PathBuf>,
    #[serde(skip)]
    texture: Option<TextureHandle>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            label: "Hello World!".to_owned(),
            value: 2.7,
            selected_file: None,
            file_list: vec![],
            temp_dir: None,
            extracted_file: None,
            texture: None,
        }
    }
}

impl TemplateApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }
        Default::default()
    }

    fn print_choose_file(&self, file: &str) {
        println!("Choose file: {}", file);
    }

    fn extract_file_from_archive(
        &mut self,
        archive_path: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        let mut file = File::open(archive_path)?;
        let len = file.metadata()?.len();
        let password = Password::empty();
        let archive = Archive::read(&mut file, len, password.as_slice()).unwrap();
        let folder_count = archive.folders.len();

        for folder_index in 0..folder_count {
            let mut file = File::open(archive_path)?;
            let folder_dec =
                BlockDecoder::new(folder_index, &archive, password.as_slice(), &mut file);

            if !folder_dec
                .entries()
                .iter()
                .any(|entry| entry.name() == file_name)
            {
                // skip the folder if it does not contain the file we want
                continue;
            }

            // 创建或获取现有的临时目录
            let temp_dir = self
                .temp_dir
                .get_or_insert_with(|| tempdir().expect("Failed to create temp dir"));
            let dest_path = temp_dir.path().join(file_name);

            folder_dec
                .for_each_entries(&mut |entry, reader| {
                    if entry.name() == file_name {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            sevenz_rust::default_entry_extract_fn(entry, reader, &dest_path)?;
                        }
                    } else {
                        // skip other files
                        std::io::copy(reader, &mut std::io::sink())?;
                    }
                    Ok(true)
                })
                .expect("ok");

            return Ok(dest_path);
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Specified file not found in archive",
        ))
    }

    fn list_files_in_archive(&mut self, archive_path: &str) -> Result<()> {
        let mut file = File::open(archive_path).unwrap();
        let len = file.metadata().unwrap().len();
        let password = Password::empty();
        let archive = Archive::read(&mut file, len, password.as_slice()).unwrap();
        let folder_count = archive.folders.len();

        self.file_list.clear();

        for folder_index in 0..folder_count {
            let mut file = File::open(archive_path).unwrap();
            let folder_dec =
                BlockDecoder::new(folder_index, &archive, password.as_slice(), &mut file);

            folder_dec
                .for_each_entries(&mut |entry, _reader| {
                    self.file_list.push(entry.name().to_owned());
                    Ok(true)
                })
                .expect("ok");
        }
        Ok(())
    }

    fn load_texture_from_path(
        &mut self,
        ctx: &egui::Context,
        path: &PathBuf,
    ) -> Option<TextureHandle> {
        let image = ImageReader::open(path).ok()?.decode().ok()?;
        let size = [image.width() as _, image.height() as _];
        let image_buffer = image.to_rgba8();
        let pixels = image_buffer.as_flat_samples();
        let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        Some(ctx.load_texture("extracted_image", color_image, Default::default()))
    }
}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_dark_light_mode_buttons(ui);
            });
        });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });

        if let Some(texture) = &self.texture {
            egui::SidePanel::right("image_panel").show(ctx, |ui| {
                let original_size = texture.size_vec2();
                let original_width = original_size.x;
                let original_height = original_size.y;
                let max_width = 640.0;
                let max_height = 480.0;

                let (final_width, final_height) =
                    if original_width <= max_width && original_height <= max_height {
                        // 如果图片本身就小于最大宽度和高度，则使用原始尺寸
                        (original_width, original_height)
                    } else {
                        // 否则按比例缩放
                        let (new_width, new_height) = if original_width > original_height {
                            let scale_factor = max_width / original_width;
                            (max_width, original_height * scale_factor)
                        } else {
                            let scale_factor = max_height / original_height;
                            (original_width * scale_factor, max_height)
                        };

                        // 确保缩放后的尺寸不超过最大值
                        if new_width > max_width || new_height > max_height {
                            (new_width.min(max_width), new_height.min(max_height))
                        } else {
                            (new_width, new_height)
                        }
                    };
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::Image::from_texture(texture)
                            .fit_to_exact_size(egui::Vec2::new(final_width, final_height))
                            .maintain_aspect_ratio(true),
                    );
                });
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label);
            });

            ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Select File").clicked() {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if let Some(file) = FileDialog::new().pick_file() {
                            self.selected_file = Some(file.clone());
                            self.list_files_in_archive(file.to_str().unwrap())
                                .expect("Failed to list files");
                        }
                    }
                }
                if let Some(selected_file) = &self.selected_file {
                    ui.label(format!("Selected file: {:?}", selected_file));
                }
            });

            // egui::ScrollArea::vertical().show(ui, |ui| {
            //     for file in &self.file_list {
            //         ui.label(file);
            //     }
            // });

            // 在闭包外部借用 `file_list` 和 `selected_file`
            let file_list = self.file_list.clone();
            let selected_file = self.selected_file.clone();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for select_file in &file_list {
                    ui.horizontal(|ui| {
                        if ui.button(select_file).clicked() {
                            // 使用克隆的 `selected_file`
                            if let Some(archive_path) = &selected_file {
                                let extracted_path = self
                                    .extract_file_from_archive(
                                        archive_path.to_str().unwrap(),
                                        select_file,
                                    )
                                    .expect("Failed to extract file");
                                self.print_choose_file(select_file);
                                println!(
                                    "Extracted to: {}",
                                    extracted_path.to_str().unwrap_or("Invalid path")
                                );
                                if select_file.ends_with(".png")
                                    || select_file.ends_with(".jpg")
                                    || select_file.ends_with(".jpeg")
                                {
                                    self.texture =
                                        self.load_texture_from_path(ctx, &extracted_path);
                                }
                                self.extracted_file = Some(extracted_path);
                            }
                        }
                    });
                }
            });
        });
        // ui.add(egui::github_link_file!(
        //     "https://github.com/emilk/eframe_template/blob/main/",
        //     "Source code."
        // ));
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
