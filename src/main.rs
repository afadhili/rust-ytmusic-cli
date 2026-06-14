mod app;
mod services;
mod tui;
mod types;
mod ui;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut app = App::new().await?;
    app.run().await
}
