use eframe::egui;

pub async fn run() {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        env!("CARGO_PKG_NAME"),
        native_options,
        Box::new(|creation_context| Box::new(Application::new(creation_context))),
    );
}

pub struct Application;

impl Application {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        Self::setup_fonts(&creation_context.egui_ctx);

        Self
    }

    pub fn setup_fonts(context: &egui::Context) {
        let mut fonts = egui::FontDefinitions::empty();
        let quicksand = String::from("quicksand");

        fonts.font_data.insert(
            quicksand.clone(),
            egui::FontData::from_static(include_bytes!("../assets/fonts/quicksand-regular.ttf")),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, quicksand.clone());

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, quicksand.clone());

        context.set_fonts(fonts);
    }
}

impl eframe::App for Application {
    fn update(&mut self, context: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.label("hi");
                ui.label("world");
            });
        });
    }
}
