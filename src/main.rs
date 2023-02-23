#![feature(let_chains)]

pub mod config;
pub mod discord;
pub mod time;

#[cfg(feature = "gui")]
pub mod gui;

#[cfg(feature = "tui")]
pub mod tui;

#[tokio::main]
pub async fn main() {
    #[cfg(feature = "gui")]
    {
        gui::run().await.unwrap();
    }

    #[cfg(feature = "tui")]
    {
        tui::run().await.unwrap();
    }
}
