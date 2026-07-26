//! Consumer web session Model providers (operator-supplied cookies/tokens).
//!
//! Unofficial wire formats — fixture-locked; see ADR 0010.

mod auth;
mod chatgpt;
mod error;
mod grok;
mod parse;

pub use auth::{
    load_secret, load_secret_pair, read_headers_file, ConsumerWebAuth, ConsumerWebConfig,
};
pub use chatgpt::ChatGptWebProvider;
pub use error::redact_secrets;
pub use grok::GrokWebProvider;
