mod account;
mod alerts;
mod app;
mod home;
mod layout;
mod lists;
mod market;
mod news;
mod recent;
mod socket;
#[cfg(test)]
mod testing;
mod theme;
mod visitor;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    corten::serve(app::app(market::Market::new())).await
}
