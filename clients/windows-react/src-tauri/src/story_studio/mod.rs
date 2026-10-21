mod domain;
mod media;
mod projection;
mod store;

pub(crate) use media::{story_studio_protocol, StoryStudioManager};

#[cfg(test)]
mod tests;
