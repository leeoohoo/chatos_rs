use crate::{
    domain::{
        datasource::DataSource,
        metadata::{MetadataNode, MetadataNodeType, MetadataNodesResponse},
    },
    error::AppResult,
};

use super::{
    super::connection::{connect_client, map_db_error},
    common::{
        make_db_node, paginate_nodes, parse_database_node, parse_schema_node, parse_table_node,
    },
    dbs::list_databases,
};

pub async fn list_nodes(
    datasource: &DataSource,
    parent_id: Option<&str>,
    page: u32,
    page_size: u32,
) -> AppResult<MetadataNodesResponse> {
    let parent = parent_id.unwrap_or("root");

    let items = if parent == "root" {
        list_database_nodes(datasource).await?
    } else if let Some(database) = parse_database_node(parent) {
        list_schema_nodes(datasource, &database).await?
    } else if let Some((database, schema)) = parse_schema_node(parent) {
        list_schema_children(datasource, &database, &schema).await?
    } else if let Some((database, schema, table)) = parse_table_node(parent) {
        list_table_children(datasource, &database, &schema, &table).await?
    } else {
        Vec::new()
    };

    Ok(paginate_nodes(items, page, page_size))
}

async fn list_database_nodes(datasource: &DataSource) -> AppResult<Vec<MetadataNode>> {
    let databases = list_databases(datasource, None, 1, 10_000).await?.items;
    Ok(databases
        .into_iter()
        .map(|database| make_db_node(&database.name))
        .collect())
}

async fn list_schema_nodes(
    datasource: &DataSource,
    database: &str,
) -> AppResult<Vec<MetadataNode>> {
    let mut client = connect_client(datasource, Some(database)).await?;
    let rows = client
        .query(
            "select name from sys.schemas
             where name not in ('sys', 'INFORMATION_SCHEMA')
             order by name",
            &[],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let schema = row.get::<&str, _>(0).unwrap_or_default().to_string();
            MetadataNode {
                id: format!("schema:{database}:{schema}"),
                parent_id: format!("db:{database}"),
                node_type: MetadataNodeType::Schema,
                display_name: schema.clone(),
                path: format!("{database}.{schema}"),
                has_children: true,
            }
        })
        .collect())
}

async fn list_schema_children(
    datasource: &DataSource,
    database: &str,
    schema: &str,
) -> AppResult<Vec<MetadataNode>> {
    let mut client = connect_client(datasource, Some(database)).await?;

    let table_rows = client
        .query(
            "select t.name
             from sys.tables t
             join sys.schemas s on s.schema_id = t.schema_id
             where s.name = @P1 and t.is_ms_shipped = 0
             order by t.name",
            &[&schema],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let view_rows = client
        .query(
            "select v.name
             from sys.views v
             join sys.schemas s on s.schema_id = v.schema_id
             where s.name = @P1 and v.is_ms_shipped = 0
             order by v.name",
            &[&schema],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let procedure_rows = client
        .query(
            "select p.name
             from sys.procedures p
             join sys.schemas s on s.schema_id = p.schema_id
             where s.name = @P1 and p.is_ms_shipped = 0
             order by p.name",
            &[&schema],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let function_rows = client
        .query(
            "select o.name
             from sys.objects o
             join sys.schemas s on s.schema_id = o.schema_id
             where s.name = @P1
               and o.type in ('FN', 'IF', 'TF', 'FS', 'FT')
               and o.is_ms_shipped = 0
             order by o.name",
            &[&schema],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let sequence_rows = client
        .query(
            "select seq.name
             from sys.sequences seq
             join sys.schemas s on s.schema_id = seq.schema_id
             where s.name = @P1
             order by seq.name",
            &[&schema],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let synonym_rows = client
        .query(
            "select syn.name
             from sys.synonyms syn
             join sys.schemas s on s.schema_id = syn.schema_id
             where s.name = @P1
             order by syn.name",
            &[&schema],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut nodes = table_rows
        .into_iter()
        .map(|row| {
            let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
            MetadataNode {
                id: format!("table:{database}:{schema}:{name}"),
                parent_id: format!("schema:{database}:{schema}"),
                node_type: MetadataNodeType::Table,
                display_name: name.clone(),
                path: format!("{database}.{schema}.{name}"),
                has_children: true,
            }
        })
        .collect::<Vec<_>>();

    nodes.extend(view_rows.into_iter().map(|row| {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        MetadataNode {
            id: format!("view:{database}:{schema}:{name}"),
            parent_id: format!("schema:{database}:{schema}"),
            node_type: MetadataNodeType::View,
            display_name: name.clone(),
            path: format!("{database}.{schema}.{name}"),
            has_children: false,
        }
    }));

    nodes.extend(procedure_rows.into_iter().map(|row| {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        MetadataNode {
            id: format!("procedure:{database}:{schema}:{name}"),
            parent_id: format!("schema:{database}:{schema}"),
            node_type: MetadataNodeType::Procedure,
            display_name: name.clone(),
            path: format!("{database}.{schema}.{name}"),
            has_children: false,
        }
    }));

    nodes.extend(function_rows.into_iter().map(|row| {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        MetadataNode {
            id: format!("function:{database}:{schema}:{name}"),
            parent_id: format!("schema:{database}:{schema}"),
            node_type: MetadataNodeType::Function,
            display_name: name.clone(),
            path: format!("{database}.{schema}.{name}"),
            has_children: false,
        }
    }));

    nodes.extend(sequence_rows.into_iter().map(|row| {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        MetadataNode {
            id: format!("sequence:{database}:{schema}:{name}"),
            parent_id: format!("schema:{database}:{schema}"),
            node_type: MetadataNodeType::Sequence,
            display_name: name.clone(),
            path: format!("{database}.{schema}.{name}"),
            has_children: false,
        }
    }));

    nodes.extend(synonym_rows.into_iter().map(|row| {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        MetadataNode {
            id: format!("synonym:{database}:{schema}:{name}"),
            parent_id: format!("schema:{database}:{schema}"),
            node_type: MetadataNodeType::Synonym,
            display_name: name.clone(),
            path: format!("{database}.{schema}.{name}"),
            has_children: false,
        }
    }));

    Ok(nodes)
}

async fn list_table_children(
    datasource: &DataSource,
    database: &str,
    schema: &str,
    table: &str,
) -> AppResult<Vec<MetadataNode>> {
    let mut client = connect_client(datasource, Some(database)).await?;
    let index_rows = client
        .query(
            "select i.name
             from sys.indexes i
             join sys.tables t on t.object_id = i.object_id
             join sys.schemas s on s.schema_id = t.schema_id
             where s.name = @P1 and t.name = @P2
               and i.index_id > 0 and i.is_hypothetical = 0
             order by i.name",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let trigger_rows = client
        .query(
            "select tr.name
             from sys.triggers tr
             join sys.tables t on t.object_id = tr.parent_id
             join sys.schemas s on s.schema_id = t.schema_id
             where s.name = @P1 and t.name = @P2
             order by tr.name",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut nodes = index_rows
        .into_iter()
        .map(|row| {
            let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
            MetadataNode {
                id: format!("index:{database}:{schema}:{table}:{name}"),
                parent_id: format!("table:{database}:{schema}:{table}"),
                node_type: MetadataNodeType::Index,
                display_name: name.clone(),
                path: format!("{database}.{schema}.{table}.{name}"),
                has_children: false,
            }
        })
        .collect::<Vec<_>>();

    nodes.extend(trigger_rows.into_iter().map(|row| {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        MetadataNode {
            id: format!("trigger:{database}:{schema}:{table}:{name}"),
            parent_id: format!("table:{database}:{schema}:{table}"),
            node_type: MetadataNodeType::Trigger,
            display_name: name.clone(),
            path: format!("{database}.{schema}.{table}.{name}"),
            has_children: false,
        }
    }));

    Ok(nodes)
}
