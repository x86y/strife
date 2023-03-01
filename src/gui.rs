use crate::config::Config;
use crate::discord::Client;
use crate::time::display_timestamp;
use bytes::Bytes;
use eframe::egui::{self, scroll_area};
use egui::FontFamily;
use egui::{Color32, FontId, RichText};
use palette::rgb::channels;
use palette::Srgba;
use reqwest::get;
use std::collections::HashMap;
use std::env;
use std::future::IntoFuture;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use strife_discord::model::id;
use strife_discord::ResponseQueue;
use tokio::sync::mpsc::Sender;

type TMutex<T> = tokio::sync::Mutex<T>;

#[derive(Debug, Default, Clone)]
struct Message {
    id: u64,
    is_header: bool,
    edited: bool,
    username: String,
    role_col: u32,
    content: String,
    attachments: Vec<String>,
    timestamp: String,
}
type Messages = Vec<Message>;
type MsgAttachments = HashMap<u64, Vec<Bytes>>;

#[derive(Default)]
struct EditableMessages {
    vals: HashMap<usize, String>,
}

/// Run the GUI version of strife.
pub async fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(600.0, 800.0)),
        ..Default::default()
    };
    let cfg = Config::load().await.unwrap();
    let mut discord = Client::new(cfg.discord_key);
    discord.current_channel = Some(id::Id::new(1051637259541159999));

    eframe::run_native(
        env!("CARGO_PKG_NAME"),
        options,
        Box::new(move |cc| {
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            let (tx1, mut rx1) = tokio::sync::mpsc::channel(1);
            let msgs = Arc::new(Mutex::new(vec![]));
            let msgsp = msgs.clone();
            let ctxp = Arc::new(Mutex::new(cc.egui_ctx.clone()));
            let loaded = Arc::new(AtomicBool::new(false));
            let loadedp = loaded.clone();
            let media_store: Arc<TMutex<MsgAttachments>> = Arc::new(TMutex::new(HashMap::new()));
            let editables = Arc::new(Mutex::new(EditableMessages::default()));
            let editablesp = editables.clone();
            let msgsp = msgs.clone();
            let app = Application::new(cc, msgs, loaded, tx, media_store.clone(), editables);
            let mut create_message_queue = ResponseQueue::default();
            tokio::spawn(async move {
                loop {
                    let discord_event = discord.next_event();
                    let create_message_queue = &mut create_message_queue;
                    let ch = rx.recv();
                    let ch1 = rx1.recv();

                    tokio::select! {
                        _maybe_event = discord_event => {
                           loadedp.store(true, std::sync::atomic::Ordering::Relaxed);
                            ctxp.lock().unwrap().request_repaint();
                        },
                        send_val = ch => {
                            if let Some(send_val) = send_val {
                            if send_val.starts_with("EDIT|") { // refactor this with Actions from
                                                               // ChannelPicker later
                                let mut sp = send_val.split('|');
                                sp.next();
                                let message_id = sp.next().unwrap();
                                let new_content = sp.last().unwrap();
                                if new_content.is_empty() {
                                    let _ = discord
                                        .rest
                                        .delete_message(discord.current_channel.unwrap(), id::Id::new(message_id.parse::<u64>().unwrap()))
                                        .into_future().await;
                                } else {
                                    let future = discord
                                        .rest
                                        .update_message(discord.current_channel.unwrap(), id::Id::new(message_id.parse::<u64>().unwrap()))
                                        .content(Some(new_content))
                                        .unwrap()
                                        .into_future();
                                    create_message_queue.push(future);
                                    create_message_queue.await.unwrap();
                                }
                            } else {
                                let future = discord
                                    .rest
                                    .create_message(discord.current_channel.unwrap())
                                    .content(&send_val)
                                    .unwrap()
                                    .into_future();
                                create_message_queue.push(future);
                                create_message_queue.await.unwrap();
                            }
                            };
                        },
                        img_fut = ch1 => {
                            if let Some(fut) = img_fut {
                                fut.await;
                            }
                        }
                    };
                    let Some(current_channel) = discord.current_channel else {
                            continue;
                        };
                    let Some(message_ids) = discord
                            .cache
                            .channel_messages(current_channel) else {
                                continue;
                        };
                    let (items, _last_id) = message_ids.iter().rev().fold(
                        (Vec::new(), None),
                        |(mut items, mut last_id), message_id| {
                            let message = discord.cache.message(*message_id).unwrap();
                            let last = message.content().to_string();
                            let author_id = message.author();
                            let user = discord.cache.user(author_id).unwrap();
                            let member = message.member().unwrap();
                            let name = member.nick.clone().unwrap_or_else(|| user.name.clone());
                            let mut roles = member
                                .roles
                                .iter()
                                .flat_map(|role_id| discord.cache.role(*role_id))
                                .filter(|role| role.color != 0)
                                .collect::<Vec<_>>();
                            roles.sort_unstable_by_key(|role| role.position);
                            let show_header = !last_id
                                .replace(author_id)
                                .map(|old_author_id| old_author_id == author_id)
                                .unwrap_or_default();
                            let rcolor = if let Some(r) = roles.last() {
                                r.color
                            } else {
                                0
                            };
                            let mut m = Message {
                                id: (*message_id).into(),
                                username: name.clone(),
                                content: last.clone(),
                                role_col: rcolor,
                                ..Default::default()
                            };
                            if name == "mori" {
                                editablesp
                                    .lock()
                                    .unwrap()
                                    .vals
                                    .insert(message_id.get() as usize, last);
                            }
                            if show_header {
                                m.is_header = true;
                                m.timestamp = display_timestamp(message.timestamp());
                            }
                            if let Some(timestamp) = message.edited_timestamp() {
                                let edited_timestamp = display_timestamp(timestamp);
                                m.edited = true;
                                m.timestamp = edited_timestamp;
                            }

                            m.attachments = message
                                .attachments()
                                .iter()
                                .map(|a| a.url.clone())
                                .collect();

                            items.push(m);
                            (items, last_id)
                        },
                    );

                    for item in items.clone() {
                        let ats = item.attachments.clone();
                        for url in ats.into_iter() {
                            let media_store = media_store.clone();
                            let mm = media_store.clone();
                            let mm = mm.lock().await;
                            if mm.get(&item.id).is_none() {
                                let _ = tx1
                                    .send(async move {
                                        let mut mm = media_store.lock().await;
                                        let r = get(url.clone()).await.unwrap();
                                        let r = r.bytes().await.unwrap();
                                        mm.entry(item.id).or_insert_with(|| vec![r]);
                                    })
                                    .await;
                            }
                        }
                    }

                    *msgsp.lock().unwrap() = items;
                    ctxp.lock().unwrap().request_repaint();
                }
            });
            Box::new(app)
        }),
    )
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "cartograph".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/cartograph.ttf")),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "cartograph".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "cartograph".to_owned());
    ctx.set_fonts(fonts);
}

struct Application {
    input_val: String,
    messages: Arc<Mutex<Messages>>,
    loaded: Arc<AtomicBool>,
    txp: Sender<String>,
    media: Arc<TMutex<MsgAttachments>>,
    editables: Arc<Mutex<EditableMessages>>,
}

impl Application {
    fn new(
        cc: &eframe::CreationContext<'_>,
        messages: Arc<Mutex<Messages>>,
        loaded: Arc<AtomicBool>,
        txp: Sender<String>,
        media: Arc<TMutex<MsgAttachments>>,
        editables: Arc<Mutex<EditableMessages>>,
    ) -> Self {
        setup_fonts(&cc.egui_ctx);
        Self {
            input_val: String::new(),
            messages,
            loaded,
            txp,
            media,
            editables,
        }
    }
}

pub fn from_u32(color: u32) -> Color32 {
    let rgba: Srgba<u8> = Srgba::from_u32::<channels::Argb>(color);
    let [r, g, b]: [u8; 3] = palette::Pixel::into_raw(rgba.color);
    Color32::from_rgb(r, g, b)
}

fn username(ui: &mut egui::Ui, m: &Message) {
    ui.heading(
        RichText::new(m.username.clone())
            .color(from_u32(m.role_col))
            .size(16.0),
    );
}
fn timestamp(ui: &mut egui::Ui, t: String) {
    ui.heading(
        RichText::new(t)
            .color(Color32::from_rgb(170, 177, 190))
            .size(16.0),
    );
}
fn content(
    ui: &mut egui::Ui,
    c: &Message,
    tx: &mut Sender<String>,
    editables: &mut HashMap<usize, String>,
    id: usize,
) {
    if c.username == "mori" {
        if let Some(es) = editables.get_mut(&id) {
            let editable = ui.add(
                egui::TextEdit::singleline(es)
                    .desired_rows(1)
                    .font(FontId::new(16.0, FontFamily::Proportional)) // for cursor height
                    .frame(false),
            );
            if editable.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                {
                    let val = es.clone();
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(format!("EDIT|{id}|{val}")).await;
                    });
                }
            }
        }
    } else {
        ui.heading(
            RichText::new(c.content.clone())
                .color(Color32::WHITE)
                .size(16.0),
        );
    }
}

fn msg(
    ui: &mut egui::Ui,
    m: &Message,
    mstore: Arc<TMutex<MsgAttachments>>,
    tx: &mut Sender<String>,
    editables: &mut HashMap<usize, String>,
) {
    ui.vertical(|ui| {
        if m.is_header {
            ui.horizontal_wrapped(|ui| {
                username(ui, m);
                timestamp(ui, m.timestamp.clone());
            });
        }
        ui.add_space(2.0);
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            content(ui, m, tx, editables, m.id as usize);
            if m.edited {
                timestamp(ui, format!("(+ @{})", m.timestamp));
            }
            if let Ok(ms) = mstore.try_lock() {
                if let Some(k) = ms.get(&m.id) {
                    for resp in k.iter() {
                        if let Ok(img) = egui_extras::RetainedImage::from_image_bytes("img", resp) {
                            img.show(ui);
                        }
                    }
                }
            }
        })
    });
}

fn input(ui: &mut egui::Ui, val: &mut String, tx: &mut Sender<String>) {
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
            {
                let val = val.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(val).await;
                });
            }
            val.clear();
            multiline.request_focus();
        }
    });
}

impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let msgs = self.messages.lock().unwrap();
        let editables = &mut self.editables.lock().unwrap().vals;
        let media = &self.media;
        if self.loaded.load(std::sync::atomic::Ordering::Relaxed) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| {
                    scroll_area::ScrollArea::vertical().show(ui, |ui| {
                        msgs.iter()
                            .for_each(|m| msg(ui, m, media.clone(), &mut self.txp, editables));
                        ui.add_space(10.0);
                    });
                    input(ui, &mut self.input_val, &mut self.txp);
                })
            });
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.with_layout(
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| ui.label(RichText::new("loading...").size(30.0)),
                )
            });
        }
    }
}
