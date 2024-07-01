use std::fs::File;
use std::fmt;
use std::error::Error;

use log::SetLoggerError;
use simplelog::{format_description, CombinedLogger, ConfigBuilder, LevelFilter, TermLogger, WriteLogger, Config, TerminalMode, ColorChoice};

pub fn init_logger() -> Result<(), InitLogError> {
    CombinedLogger::init(
        vec![
            TermLogger::new(
                LevelFilter::Info,
                Config::default(),
                TerminalMode::Stderr,
                ColorChoice::Auto
            ),
            WriteLogger::new(
                LevelFilter::Debug,
                ConfigBuilder::new()
                    .set_time_format_custom(
                        format_description!("[hour]:[minute]:[second].[subsecond digits:3]")
                    )
                    .add_filter_allow_str("wiki_utils")
                    .build(),
                File::create("wiki-link-calc-debug.log")?
            ),
        ]
    )?;

    Ok(())
}

#[derive(Debug)]
pub enum InitLogError {
    LogError(SetLoggerError),
    FileCreateError(std::io::Error),
}


impl fmt::Display for InitLogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //TODO
        write!(f, "InitLogError")
    }
}

impl From<SetLoggerError> for InitLogError {
    fn from(e: SetLoggerError) -> InitLogError {
        InitLogError::LogError(e)
    }
}
impl From<std::io::Error> for InitLogError {
    fn from(e: std::io::Error) -> InitLogError {
        InitLogError::FileCreateError(e)
    }
}

impl Error for InitLogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LogError(e) => Some(e),
            Self::FileCreateError(e) => Some(e),
        }
    }
}
