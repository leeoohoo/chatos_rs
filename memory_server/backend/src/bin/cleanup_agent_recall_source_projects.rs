use mongodb::bson::{doc, Document};
use mongodb::Collection;

#[path = "../bin_support/mongo_maintenance.rs"]
mod mongo_maintenance;

#[derive(Debug, Clone)]
struct CliArgs {
    mongo: mongo_maintenance::MongoCliArgs,
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args()?;
    mongo_maintenance::print_mongo_cli_header("CLEANUP", &args.mongo);

    let db = mongo_maintenance::connect_database(&args.mongo.target, "memory_agent_recall_cleanup")
        .await?;

    let coll: Collection<Document> = db.collection("agent_recalls");
    let filter = doc! {
        "source_project_ids": { "$exists": true }
    };

    let before = coll
        .count_documents(filter.clone())
        .await
        .map_err(|e| e.to_string())?;
    println!("[CLEANUP] matched before = {}", before);

    if args.dry_run || before == 0 {
        println!("[CLEANUP] no changes applied");
        return Ok(());
    }

    let result = coll
        .update_many(
            filter.clone(),
            doc! { "$unset": { "source_project_ids": "" } },
        )
        .await
        .map_err(|e| e.to_string())?;

    let after = coll
        .count_documents(filter)
        .await
        .map_err(|e| e.to_string())?;

    println!("[CLEANUP] matched = {}", result.matched_count);
    println!("[CLEANUP] modified = {}", result.modified_count);
    println!("[CLEANUP] remaining = {}", after);
    Ok(())
}

fn parse_args() -> Result<CliArgs, String> {
    let mongo = mongo_maintenance::parse_mongo_cli_args("cleanup_agent_recall_source_projects")?;
    Ok(CliArgs {
        dry_run: mongo.dry_run,
        mongo,
    })
}
