use std::fmt::{Debug, Display};

#[derive(Debug)]
pub enum ChoraError {
    FailedToFindAdapter{},
    FailedGettingSuitableDevice{},
}

impl std::error::Error for ChoraError {}

impl Display for ChoraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self, f)
    }
}

