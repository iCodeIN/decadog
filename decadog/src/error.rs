use std::io::Error as IoError;

use config::ConfigError;
use decadog_core::Error as DecadogError;
use scout::errors::Error as ScoutError;
use snafu::Snafu;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum Error {
    #[snafu(display("Config error: {}", source))]
    Config { source: ConfigError },

    #[snafu(display("Decadog client error: {}", source))]
    Decadog { source: DecadogError },

    #[snafu(display("Scout error: {}", source))]
    Scout { source: ScoutError },

    #[snafu(display("Io error: {}", source))]
    Io { source: IoError },

    #[snafu(display("User error: {}", description))]
    User { description: String },

    #[snafu(display("Invalid settings: {}", description))]
    Settings { description: String },
}

impl From<ConfigError> for Error {
    fn from(source: ConfigError) -> Self {
        Error::Config { source }
    }
}

impl From<DecadogError> for Error {
    fn from(source: DecadogError) -> Self {
        Error::Decadog { source }
    }
}

impl From<ScoutError> for Error {
    fn from(source: ScoutError) -> Self {
        Error::Scout { source }
    }
}

impl From<IoError> for Error {
    fn from(source: IoError) -> Self {
        Error::Io { source }
    }
}
