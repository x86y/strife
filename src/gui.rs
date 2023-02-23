use crate::config::Config;
use crate::discord::Client;
use crate::time::display_timestamp;
use eframe::egui::{self, scroll_area};
use egui::FontFamily;
use egui::{Color32, FontId, RichText};
use std::env;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{Receiver, Sender};
use twilight_model::id::Id;

#[derive(Debug, Default)]
struct DcMessage {
    is_header: bool,
    edited: bool,
    username: String,
    content: String,
    timestamp: String,
}
type DcMessages = Vec<DcMessage>;

/// Run the GUI version of strife.
pub async fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(600.0, 800.0)),
        ..Default::default()
    };
    let cfg = Config::load().await.unwrap();
    let mut dc = Client::new(cfg.discord_key);
    dc.current_channel = Some(Id::new(1051637259541159999));
    let h = Handle::current();

    eframe::run_native(
        env!("CARGO_PKG_NAME"),
        options,
        Box::new(move |cc| {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let (txp, mut rxp) = tokio::sync::mpsc::channel(1);
            let app = Application::new(cc, txp, rx);
            std::thread::spawn(move || {
                h.block_on(async move {
                    loop {
                        if let Ok(send_val) = rxp.try_recv() {
                            dc.rest
                                .create_message(dc.current_channel.unwrap())
                                .content(&send_val)
                                .unwrap()
                                .await
                                .unwrap();
                        };

                        let _ = dc.next_event().await;
                        let Some(current_channel) = dc.current_channel else {
                            continue;
                        };
                        let Some(message_ids) = dc
                            .cache
                            .channel_messages(current_channel) else {
                                continue;
                        };
                        let (items, _last_id) = message_ids.iter().rev().fold(
                            (Vec::new(), None),
                            |(mut items, mut last_id), message_id| {
                                let message = dc.cache.message(*message_id).unwrap();
                                let last = message.content().to_string();
                                let author_id = message.author();
                                let user = dc.cache.user(author_id).unwrap();
                                let name = message
                                    .member()
                                    .unwrap()
                                    .nick
                                    .clone()
                                    .unwrap_or_else(|| user.name.clone());
                                let show_header = !last_id
                                    .replace(author_id)
                                    .map(|old_author_id| old_author_id == author_id)
                                    .unwrap_or_default();
                                let mut m = DcMessage {
                                    username: name,
                                    content: last,
                                    ..Default::default()
                                };
                                if show_header {
                                    m.is_header = true;
                                    m.timestamp = display_timestamp(message.timestamp());
                                }
                                if let Some(timestamp) = message.edited_timestamp() {
                                    let edited_timestamp = display_timestamp(timestamp);
                                    m.edited = true;
                                    m.timestamp = edited_timestamp;
                                }
                                items.push(m);
                                (items, last_id)
                            },
                        );
                        tx.send(items).await.unwrap();
                    }
                });
            });
            Box::new(app)
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "quicksand".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/quicksand-regular.ttf")),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "quicksand".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "quicksand".to_owned());
    ctx.set_fonts(fonts);
}

struct Application {
    input_val: String,
    messages: DcMessages,
    txp: Sender<String>,
    rx: Receiver<DcMessages>,
}

impl Application {
    fn new(
        cc: &eframe::CreationContext<'_>,
        txp: Sender<String>,
        rx: Receiver<DcMessages>,
    ) -> Self {
        setup_fonts(&cc.egui_ctx);
        Self {
            input_val: String::new(),
            messages: vec![],
            txp,
            rx,
        }
    }
}

fn msg(ui: &mut egui::Ui, m: &DcMessage) {
    ui.vertical(|ui| {
        if m.is_header {
            ui.horizontal_wrapped(|ui| {
                ui.heading(RichText::new(m.username.clone()).color(Color32::from_rgb(235, 15, 35)));
                ui.heading(
                    RichText::new(m.timestamp.clone()).color(Color32::from_rgb(169, 169, 169)),
                );
            });
        }
        ui.add_space(2.0);
        if m.edited {
            ui.heading(format!(
                "{} (last edited: {})",
                m.content.clone(),
                m.timestamp
            ));
        } else {
            ui.heading(m.content.clone());
        }
    });
}

fn input(ui: &mut egui::Ui, val: &mut String, txp: &mut Sender<String>) {
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        let multiline = ui.add(
            egui::TextEdit::singleline(val)
                .code_editor()
                .font(FontId::new(24.0, FontFamily::Proportional)) // for cursor height
                .desired_rows(1)
                .desired_width(f32::INFINITY)
                .margin(egui::Vec2 { x: 0.0, y: 8.0 }),
        );
        if multiline.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            // hopeless ??
            let valc = val.clone();
            let txpc = txp.clone();
            tokio::spawn(async move {
                txpc.send(valc).await.unwrap();
            });
        }
    });
}

impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mf = egui::containers::Frame {
            fill: Color32::BLACK,
            ..Default::default()
        };
        if let Ok(ms) = self.rx.try_recv() {
            self.messages = ms;
        }
        if self.messages.is_empty() {
            egui::CentralPanel::default().frame(mf).show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| {
                    scroll_area::ScrollArea::vertical().show(ui, |ui| {
                        self.messages.iter().for_each(|m| msg(ui, m));
                        ui.add_space(10.0);
                    });
                    input(ui, &mut self.input_val, &mut self.txp);
                })
            });
        } else {
            egui::CentralPanel::default().frame(mf).show(ctx, |ui| {
                ui.with_layout(
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| ui.label(RichText::new("loading...").size(30.0)),
                )
            });
        }
    }
}
