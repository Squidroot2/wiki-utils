mod logging;

use std::error::Error;
use std::fs::File;
use std::io::Write;

use log::info;

use wiki_utils::links::LinkCalculator;
use wiki_utils::client::AsyncClient;

use crate::logging::init_logger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{

    init_logger()?;

    let client = AsyncClient::new();

    info!("Retrieving starting article");
    let article = client.get_article("Direct and indirect realism").await?;

    info!("Initializing LinkCalculator");
    let mut calc = LinkCalculator::from_article(&article)?;

    let layers = 2;
    info!("Calculating {} layers of neighbors", layers);
    calc.compute_layers_async(layers).await?;

    let file_name = article.get_article_title()? + ".txt";
    info!("Writing calc data to {}", file_name);
    File::create(file_name)?.write_all(calc.to_string().as_bytes())?;

    Ok(())
}


