mod logging;

use std::env;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::time::Instant;

use log::info;

use wiki_utils::client::AsyncClient;
use wiki_utils::links::LinkCalculator;

use crate::logging::init_logger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logger()?;

    let start = Instant::now();

    let args = Arguments::get()?;

    let result = execute_and_print(&args.starting_article, args.layers_to_calc).await;

    let elapsed = start.elapsed();
    info!("Finished in {:.3?}", elapsed);

    result
}

struct Arguments {
    starting_article: String,
    layers_to_calc: NonZeroUsize,
}

impl Arguments {
    fn get() -> Result<Self, ArgumentError> {
        let mut args = env::args();
        let _binary = args.next();
        let starting_article = args.next().ok_or(ArgumentError::MissingArgument)?;
        let layers_calc_arg = args.next().ok_or(ArgumentError::MissingArgument)?;
        let layers_to_calc = NonZeroUsize::from_str(&layers_calc_arg).map_err(|_| ArgumentError::InvalidLayerCount(layers_calc_arg))?;
        Ok(Self {
            starting_article,
            layers_to_calc,
        })
    }
}

#[derive(Debug)]
enum ArgumentError {
    MissingArgument,
    InvalidLayerCount(String),
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument => write!(f, "Too few arguments given"),
            Self::InvalidLayerCount(arg) => write!(
                f,
                "'{}' is not a valid layer count: Must be a nonzero unsigned {}-bit integer",
                arg,
                usize::BITS,
            ),
        }
    }
}

impl Error for ArgumentError {}

async fn execute_and_print(article_name: &str, layers_to_calculate: NonZeroUsize) -> Result<(), Box<dyn Error>> {
    let client = AsyncClient::new();

    info!("Retrieving starting article: {}", article_name);
    let article = client.get_article(article_name).await?;

    info!("Initializing LinkCalculator");
    let mut calc = LinkCalculator::from_article(&article)?;

    let layers = layers_to_calculate.get() - 1;
    info!("Calculating {} additonal layers of neighbors", layers);
    calc.compute_layers_async(layers).await?;

    let file_name = article.get_article_title()? + ".txt";
    info!("Writing calc data to {}", file_name);
    File::create(file_name)?.write_all(calc.to_string().as_bytes())?;

    Ok(())
}
