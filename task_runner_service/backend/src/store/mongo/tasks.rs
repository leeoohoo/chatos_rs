use super::*;

mod listing;
mod mutations;
mod prerequisites;

impl MongoStore {
    fn task_summary_projection_stage() -> Document {
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "title": 1,
                "status": 1,
                "default_model_config_id": 1,
                "creator_user_id": 1,
                "creator_username": 1,
                "creator_display_name": 1,
                "last_run_id": 1,
                "updated_at": 1,
            }
        }
    }

    fn task_summary_pipeline(
        match_stage: Option<Document>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Vec<Document> {
        let mut pipeline = Vec::new();
        if let Some(filter) = match_stage {
            pipeline.push(doc! { "$match": filter });
        }
        pipeline.push(Self::task_summary_projection_stage());
        pipeline.push(doc! { "$sort": { "updated_at": -1, "id": -1 } });
        if let Some(stage) =
            (!build_skip_stage(offset).is_empty()).then(|| build_skip_stage(offset))
        {
            pipeline.push(stage);
        }
        if let Some(stage) =
            (!build_limit_stage(limit).is_empty()).then(|| build_limit_stage(limit))
        {
            pipeline.push(stage);
        }
        pipeline
    }
}
