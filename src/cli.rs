//! The command line: its grammar, its help, and one module per command
//! family. `main` parses and dispatches; everything a command says or
//! does on the way lives here.

mod agent;
mod config;
mod grammar;
mod help;
mod install;
mod market;
mod output;
mod report;
mod setup;
mod status;

pub(crate) use agent::*;
pub(crate) use config::*;
pub(crate) use grammar::*;
pub(crate) use help::*;
pub(crate) use install::*;
pub(crate) use market::*;
pub(crate) use output::*;
pub(crate) use report::*;
pub(crate) use setup::*;
pub(crate) use status::*;
