// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

mod cancellation;
mod events;
mod listing;
mod persistence;

impl MongoStore {
    fn run_summary_projection_stage() -> Document {
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "task_id": 1,
                "status": 1,
                "model_config_id": 1,
                "updated_at": 1,
            }
        }
    }

    fn run_summary_pipeline(
        match_stage: Option<Document>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Vec<Document> {
        let mut pipeline = Vec::new();
        if let Some(filter) = match_stage {
            pipeline.push(doc! { "$match": filter });
        }
        pipeline.push(Self::run_summary_projection_stage());
        pipeline.push(doc! { "$sort": { "updated_at": -1, "id": -1 } });
        let skip_stage = build_skip_stage(offset);
        if !skip_stage.is_empty() {
            pipeline.push(skip_stage);
        }
        let limit_stage = build_limit_stage(limit);
        if !limit_stage.is_empty() {
            pipeline.push(limit_stage);
        }
        pipeline
    }
}
