#[cfg(feature = "gui")]
pub mod gui;

#[cfg(feature = "tui")]
pub mod tui;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    #[cfg(feature = "gui")]
    {
        gui::run().await;
    }

    #[cfg(feature = "tui")]
    {
        tui::run().await;
    }
}
