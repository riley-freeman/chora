use std::fmt::{Debug, Display};

use wgpu::PollError;

#[derive(Debug)]
pub enum ChoraError {
    FailedToFindAdapter{},
    FailedGettingSuitableDevice{},
    FailedToCreateSurface,
    NoSurfaceConfigured,
    FailedToAcquireSwapchainTexture,

    SubmissionPollingError{err: PollError},
}

impl std::error::Error for ChoraError {}

impl Display for ChoraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self, f)
    }
}

