use mongodb::bson::{doc, Bson, Document};

pub(crate) fn doc_i64(doc: &Document, key: &str) -> i64 {
    match doc.get(key) {
        Some(Bson::Int32(v)) => *v as i64,
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Double(v)) => *v as i64,
        _ => 0,
    }
}

pub(crate) fn summary_agent_id_expr() -> Document {
    doc! {
        "$ifNull": [
            "$session.metadata.contact.agent_id",
            {
                "$ifNull": [
                    "$session.metadata.ui_contact.agent_id",
                    {
                        "$ifNull": [
                            "$session.metadata.ui_chat_selection.selected_agent_id",
                            "$session.metadata.ui_chat_selection.selectedAgentId"
                        ]
                    }
                ]
            }
        ]
    }
}

pub(crate) fn summary_project_id_expr() -> Document {
    doc! {
        "$ifNull": [
            "$session.project_id",
            {
                "$ifNull": [
                    "$session.metadata.chat_runtime.project_id",
                    {
                        "$ifNull": [
                            "$session.metadata.chat_runtime.projectId",
                            "0"
                        ]
                    }
                ]
            }
        ]
    }
}
