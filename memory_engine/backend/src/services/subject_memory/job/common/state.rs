pub(crate) struct SubjectMemoryJobProgress {
    pub(crate) processed_count: usize,
    pub(crate) generated_level0: usize,
    pub(crate) generated_rollups: usize,
    pub(crate) marked_source_summaries: usize,
    pub(crate) marked_source_memories: usize,
}

impl SubjectMemoryJobProgress {
    pub(crate) fn new() -> Self {
        Self {
            processed_count: 0,
            generated_level0: 0,
            generated_rollups: 0,
            marked_source_summaries: 0,
            marked_source_memories: 0,
        }
    }

    pub(crate) fn output_count(&self) -> i64 {
        (self.generated_level0 + self.generated_rollups) as i64
    }

    pub(crate) fn marked_count(&self) -> usize {
        self.marked_source_summaries + self.marked_source_memories
    }

    pub(crate) fn add_processed(&mut self, count: usize) {
        self.processed_count += count;
    }
}
