#![allow(dead_code)]

use std::env;

use mongodb::bson::doc;
use mongodb::options::ClientOptions;
use mongodb::{Client, Database};

#[derive(Debug, Clone)]
pub struct MongoTargetArgs {
    pub mongo_uri: String,
    pub mongo_db: String,
}

#[derive(Debug, Clone)]
pub struct MongoCliArgs {
    pub target: MongoTargetArgs,
    pub dry_run: bool,
}

pub fn default_mongo_target() -> MongoTargetArgs {
    MongoTargetArgs {
        mongo_uri: env::var("MEMORY_SERVER_MONGODB_URI")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "mongodb://admin:admin@127.0.0.1:27018/admin".to_string()),
        mongo_db: env::var("MEMORY_SERVER_MONGODB_DATABASE")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "memory_server".to_string()),
    }
}

pub fn parse_mongo_cli_args(bin_name: &str) -> Result<MongoCliArgs, String> {
    let mut target = default_mongo_target();
    let mut dry_run = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mongo-uri" => {
                target.mongo_uri = args
                    .next()
                    .ok_or_else(|| "--mongo-uri requires value".to_string())?;
            }
            "--mongo-db" => {
                target.mongo_db = args
                    .next()
                    .ok_or_else(|| "--mongo-db requires value".to_string())?;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--help" | "-h" => {
                print_mongo_cli_usage(bin_name, true);
                std::process::exit(0);
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    Ok(MongoCliArgs { target, dry_run })
}

pub fn print_mongo_cli_usage(bin_name: &str, supports_dry_run: bool) {
    if supports_dry_run {
        println!(
            "Usage:\n  cargo run --bin {bin_name} -- [--mongo-uri <uri>] [--mongo-db <name>] [--dry-run]"
        );
    } else {
        println!("Usage:\n  cargo run --bin {bin_name} -- [--mongo-uri <uri>] [--mongo-db <name>]");
    }
}

pub fn print_mongo_cli_header(prefix: &str, args: &MongoCliArgs) {
    println!("[{prefix}] mongo uri = {}", args.target.mongo_uri);
    println!("[{prefix}] mongo db  = {}", args.target.mongo_db);
    println!("[{prefix}] dry run   = {}", args.dry_run);
}

pub async fn connect_database(
    target: &MongoTargetArgs,
    app_name: &str,
) -> Result<Database, String> {
    let mut options = ClientOptions::parse(target.mongo_uri.as_str())
        .await
        .map_err(|e| format!("invalid mongo uri: {e}"))?;
    options.app_name = Some(app_name.to_string());

    let client = Client::with_options(options).map_err(|e| e.to_string())?;
    let db = client.database(target.mongo_db.as_str());

    db.run_command(doc! {"ping": 1})
        .await
        .map_err(|e| format!("mongo ping failed: {e}"))?;

    Ok(db)
}
