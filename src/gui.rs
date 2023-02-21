pub fn run() {
    let native_options = eframe::NativeOptions::default();

    eframe::run_native(
        env!("CARGO_PKG_NAME"),
        native_options,
        Box::new(|creation_context| Box::new(Application::new(creation_context))),
    );
}

pub struct Application;

impl Application {
    pub fn new(_creation_context: &eframe::CreationContext<'_>) -> Self {
        Self
    }
}

impl eframe::App for Application {
    fn update(&mut self, context: &egui::Context, frame: &mut eframe::frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("real");
        });
    }
}
