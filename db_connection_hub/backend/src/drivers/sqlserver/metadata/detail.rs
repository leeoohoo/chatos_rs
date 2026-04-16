use crate::{
    domain::{
        datasource::DataSource,
        metadata::{
            MetadataNodeType, ObjectColumn, ObjectConstraint, ObjectDetailResponse, ObjectIndex,
        },
    },
    error::{AppError, AppResult},
};

use super::{
    super::connection::{connect_client, map_db_error},
    common::{parse_index_node, parse_trigger_node},
};

pub async fn object_detail(
    datasource: &DataSource,
    node_id: &str,
) -> AppResult<ObjectDetailResponse> {
    if let Some((node_type, database, schema, object_name, object_type_code)) =
        parse_relation_node(node_id)
    {
        return load_relation_detail(
            datasource,
            node_id,
            node_type,
            &database,
            &schema,
            &object_name,
            object_type_code,
        )
        .await;
    }

    if let Some((node_type, database, schema, object_name, kind)) = parse_special_node(node_id) {
        return load_special_detail(
            datasource,
            node_id,
            node_type,
            &database,
            &schema,
            &object_name,
            kind,
        )
        .await;
    }

    if let Some((database, schema, table, index_name)) = parse_index_node(node_id) {
        return load_index_detail(datasource, node_id, &database, &schema, &table, &index_name)
            .await;
    }

    if let Some((database, schema, table, trigger_name)) = parse_trigger_node(node_id) {
        return load_trigger_detail(
            datasource,
            node_id,
            &database,
            &schema,
            &table,
            &trigger_name,
        )
        .await;
    }

    Err(AppError::NotFound(format!(
        "unsupported sql server detail node: {node_id}"
    )))
}

async fn load_relation_detail(
    datasource: &DataSource,
    node_id: &str,
    node_type: MetadataNodeType,
    database: &str,
    schema: &str,
    object_name: &str,
    object_type_code: &str,
) -> AppResult<ObjectDetailResponse> {
    let mut client = connect_client(datasource, Some(database)).await?;

    let columns_rows = client
        .query(
            "select
                c.name as column_name,
                case
                    when typ.name in ('nvarchar', 'nchar')
                        then typ.name + '(' + case when c.max_length = -1 then 'max' else cast(c.max_length / 2 as varchar(10)) end + ')'
                    when typ.name in ('varchar', 'char', 'varbinary', 'binary')
                        then typ.name + '(' + case when c.max_length = -1 then 'max' else cast(c.max_length as varchar(10)) end + ')'
                    when typ.name in ('decimal', 'numeric')
                        then typ.name + '(' + cast(c.precision as varchar(10)) + ',' + cast(c.scale as varchar(10)) + ')'
                    when typ.name in ('datetime2', 'datetimeoffset', 'time')
                        then typ.name + '(' + cast(c.scale as varchar(10)) + ')'
                    else typ.name
                end as data_type,
                c.is_nullable
             from sys.columns c
             join sys.types typ on typ.user_type_id = c.user_type_id
             join sys.objects o on o.object_id = c.object_id
             join sys.schemas s on s.schema_id = o.schema_id
             where s.name = @P1 and o.name = @P2 and o.type = @P3
             order by c.column_id",
            &[&schema, &object_name, &object_type_code],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    if columns_rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "sql server object not found: {database}.{schema}.{object_name}"
        )));
    }

    let columns = columns_rows
        .into_iter()
        .map(|row| ObjectColumn {
            name: row.get::<&str, _>(0).unwrap_or_default().to_string(),
            data_type: row.get::<&str, _>(1).unwrap_or("unknown").to_string(),
            nullable: row.get::<bool, _>(2).unwrap_or(true),
        })
        .collect::<Vec<_>>();

    let (indexes, constraints) = if matches!(node_type, MetadataNodeType::Table) {
        (
            load_table_indexes(&mut client, schema, object_name).await?,
            load_table_constraints(&mut client, schema, object_name).await?,
        )
    } else {
        (Vec::new(), Vec::new())
    };

    let ddl = client
        .query(
            "select object_definition(o.object_id)
             from sys.objects o
             join sys.schemas s on s.schema_id = o.schema_id
             where s.name = @P1 and o.name = @P2 and o.type = @P3",
            &[&schema, &object_name, &object_type_code],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .first()
        .and_then(|row| row.get::<&str, _>(0))
        .map(std::string::ToString::to_string);

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type,
        name: object_name.to_string(),
        columns,
        indexes,
        constraints,
        ddl,
    })
}

async fn load_special_detail(
    datasource: &DataSource,
    node_id: &str,
    node_type: MetadataNodeType,
    database: &str,
    schema: &str,
    object_name: &str,
    kind: SpecialNodeKind,
) -> AppResult<ObjectDetailResponse> {
    let mut client = connect_client(datasource, Some(database)).await?;
    let rows = match kind {
        SpecialNodeKind::Procedure => client
            .query(
                "select object_definition(p.object_id) as ddl
                     from sys.procedures p
                     join sys.schemas s on s.schema_id = p.schema_id
                     where s.name = @P1 and p.name = @P2",
                &[&schema, &object_name],
            )
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?
            .into_first_result()
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?,
        SpecialNodeKind::Function => client
            .query(
                "select object_definition(o.object_id) as ddl
                     from sys.objects o
                     join sys.schemas s on s.schema_id = o.schema_id
                     where s.name = @P1 and o.name = @P2
                       and o.type in ('FN', 'IF', 'TF', 'FS', 'FT')",
                &[&schema, &object_name],
            )
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?
            .into_first_result()
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?,
        SpecialNodeKind::Sequence => client
            .query(
                "select cast(
                        'CREATE SEQUENCE [' + s.name + '].[' + seq.name + '] START WITH '
                        + cast(seq.start_value as varchar(64))
                        + ' INCREMENT BY '
                        + cast(seq.increment as varchar(64))
                        + ';' as nvarchar(max)
                     ) as ddl
                     from sys.sequences seq
                     join sys.schemas s on s.schema_id = seq.schema_id
                     where s.name = @P1 and seq.name = @P2",
                &[&schema, &object_name],
            )
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?
            .into_first_result()
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?,
        SpecialNodeKind::Synonym => client
            .query(
                "select cast(
                        'CREATE SYNONYM [' + s.name + '].[' + syn.name + '] FOR '
                        + syn.base_object_name + ';' as nvarchar(max)
                     ) as ddl
                     from sys.synonyms syn
                     join sys.schemas s on s.schema_id = syn.schema_id
                     where s.name = @P1 and syn.name = @P2",
                &[&schema, &object_name],
            )
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?
            .into_first_result()
            .await
            .map_err(|err| map_db_error("query", err.to_string()))?,
    };

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "sql server object not found: {database}.{schema}.{object_name}"
        )));
    }

    let ddl = rows
        .first()
        .and_then(|row| row.get::<&str, _>(0))
        .map(std::string::ToString::to_string);

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type,
        name: object_name.to_string(),
        columns: Vec::new(),
        indexes: Vec::new(),
        constraints: Vec::new(),
        ddl,
    })
}

async fn load_index_detail(
    datasource: &DataSource,
    node_id: &str,
    database: &str,
    schema: &str,
    table: &str,
    index_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let mut client = connect_client(datasource, Some(database)).await?;
    let rows = client
        .query(
            "select i.is_unique, ic.key_ordinal, c.name as column_name
             from sys.indexes i
             join sys.tables t on t.object_id = i.object_id
             join sys.schemas s on s.schema_id = t.schema_id
             left join sys.index_columns ic
               on ic.object_id = i.object_id
              and ic.index_id = i.index_id
              and ic.key_ordinal > 0
             left join sys.columns c
               on c.object_id = ic.object_id
              and c.column_id = ic.column_id
             where s.name = @P1 and t.name = @P2 and i.name = @P3
               and i.index_id > 0 and i.is_hypothetical = 0
             order by ic.key_ordinal",
            &[&schema, &table, &index_name],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "sql server index not found: {database}.{schema}.{table}.{index_name}"
        )));
    }

    let is_unique = rows
        .first()
        .and_then(|row| row.get::<bool, _>(0))
        .unwrap_or(false);
    let columns = rows
        .into_iter()
        .filter_map(|row| row.get::<&str, _>(2).map(std::string::ToString::to_string))
        .collect::<Vec<_>>();

    let ddl = Some(format!(
        "CREATE {}INDEX [{}] ON [{}].[{}] ({});",
        if is_unique { "UNIQUE " } else { "" },
        index_name,
        schema,
        table,
        columns
            .iter()
            .map(|column| format!("[{column}]"))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type: MetadataNodeType::Index,
        name: index_name.to_string(),
        columns: Vec::new(),
        indexes: vec![ObjectIndex {
            name: index_name.to_string(),
            columns,
            is_unique,
        }],
        constraints: Vec::new(),
        ddl,
    })
}

async fn load_trigger_detail(
    datasource: &DataSource,
    node_id: &str,
    database: &str,
    schema: &str,
    table: &str,
    trigger_name: &str,
) -> AppResult<ObjectDetailResponse> {
    let mut client = connect_client(datasource, Some(database)).await?;
    let rows = client
        .query(
            "select object_definition(tr.object_id) as trigger_definition
             from sys.triggers tr
             join sys.tables t on t.object_id = tr.parent_id
             join sys.schemas s on s.schema_id = t.schema_id
             where s.name = @P1 and t.name = @P2 and tr.name = @P3",
            &[&schema, &table, &trigger_name],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let ddl = rows
        .first()
        .and_then(|row| row.get::<&str, _>(0))
        .map(std::string::ToString::to_string);

    if ddl.is_none() {
        return Err(AppError::NotFound(format!(
            "sql server trigger not found: {database}.{schema}.{table}.{trigger_name}"
        )));
    }

    Ok(ObjectDetailResponse {
        node_id: node_id.to_string(),
        node_type: MetadataNodeType::Trigger,
        name: trigger_name.to_string(),
        columns: Vec::new(),
        indexes: Vec::new(),
        constraints: Vec::new(),
        ddl,
    })
}

async fn load_table_indexes(
    client: &mut super::super::connection::SqlServerClient,
    schema: &str,
    table: &str,
) -> AppResult<Vec<ObjectIndex>> {
    let rows = client
        .query(
            "select i.name as index_name, i.is_unique, ic.key_ordinal, c.name as column_name
             from sys.indexes i
             join sys.tables t on t.object_id = i.object_id
             join sys.schemas s on s.schema_id = t.schema_id
             join sys.index_columns ic
               on ic.object_id = i.object_id
              and ic.index_id = i.index_id
              and ic.key_ordinal > 0
             join sys.columns c
               on c.object_id = ic.object_id
              and c.column_id = ic.column_id
             where s.name = @P1 and t.name = @P2
               and i.index_id > 0 and i.is_hypothetical = 0
             order by i.name, ic.key_ordinal",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let mut indexes: Vec<ObjectIndex> = Vec::new();
    for row in rows {
        let index_name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        let is_unique = row.get::<bool, _>(1).unwrap_or(false);
        let column_name = row.get::<&str, _>(3).unwrap_or_default().to_string();

        if let Some(existing) = indexes.iter_mut().find(|item| item.name == index_name) {
            existing.columns.push(column_name);
        } else {
            indexes.push(ObjectIndex {
                name: index_name,
                columns: vec![column_name],
                is_unique,
            });
        }
    }

    Ok(indexes)
}

async fn load_table_constraints(
    client: &mut super::super::connection::SqlServerClient,
    schema: &str,
    table: &str,
) -> AppResult<Vec<ObjectConstraint>> {
    let mut constraints = Vec::new();

    let key_rows = client
        .query(
            "select kc.name as constraint_name, kc.type_desc as constraint_type, c.name as column_name
             from sys.key_constraints kc
             join sys.tables t on t.object_id = kc.parent_object_id
             join sys.schemas s on s.schema_id = t.schema_id
             join sys.index_columns ic
               on ic.object_id = kc.parent_object_id
              and ic.index_id = kc.unique_index_id
              and ic.key_ordinal > 0
             join sys.columns c
               on c.object_id = ic.object_id
              and c.column_id = ic.column_id
             where s.name = @P1 and t.name = @P2
             order by kc.name, ic.key_ordinal",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    append_constraints(&mut constraints, key_rows);

    let foreign_key_rows = client
        .query(
            "select fk.name as constraint_name, cast('FOREIGN KEY' as varchar(32)) as constraint_type, c.name as column_name
             from sys.foreign_keys fk
             join sys.tables t on t.object_id = fk.parent_object_id
             join sys.schemas s on s.schema_id = t.schema_id
             join sys.foreign_key_columns fkc on fkc.constraint_object_id = fk.object_id
             join sys.columns c
               on c.object_id = fkc.parent_object_id
              and c.column_id = fkc.parent_column_id
             where s.name = @P1 and t.name = @P2
             order by fk.name, fkc.constraint_column_id",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    append_constraints(&mut constraints, foreign_key_rows);

    let check_rows = client
        .query(
            "select cc.name as constraint_name, cast('CHECK' as varchar(32)) as constraint_type, cast(null as varchar(256)) as column_name
             from sys.check_constraints cc
             join sys.tables t on t.object_id = cc.parent_object_id
             join sys.schemas s on s.schema_id = t.schema_id
             where s.name = @P1 and t.name = @P2
             order by cc.name",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    append_constraints(&mut constraints, check_rows);

    let default_rows = client
        .query(
            "select dc.name as constraint_name, cast('DEFAULT' as varchar(32)) as constraint_type, c.name as column_name
             from sys.default_constraints dc
             join sys.tables t on t.object_id = dc.parent_object_id
             join sys.schemas s on s.schema_id = t.schema_id
             join sys.columns c
               on c.object_id = dc.parent_object_id
              and c.column_id = dc.parent_column_id
             where s.name = @P1 and t.name = @P2
             order by dc.name, c.column_id",
            &[&schema, &table],
        )
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?
        .into_first_result()
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    append_constraints(&mut constraints, default_rows);

    Ok(constraints)
}

fn append_constraints(constraints: &mut Vec<ObjectConstraint>, rows: Vec<tiberius::Row>) {
    for row in rows {
        let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
        let constraint_type = row.get::<&str, _>(1).unwrap_or("UNKNOWN").to_string();
        let column = row.get::<&str, _>(2).map(std::string::ToString::to_string);

        if let Some(existing) = constraints
            .iter_mut()
            .find(|item| item.name == name && item.constraint_type == constraint_type)
        {
            if let Some(column) = column {
                existing.columns.push(column);
            }
        } else {
            let mut columns = Vec::new();
            if let Some(column) = column {
                columns.push(column);
            }
            constraints.push(ObjectConstraint {
                name,
                constraint_type,
                columns,
            });
        }
    }
}

#[derive(Clone, Copy)]
enum SpecialNodeKind {
    Procedure,
    Function,
    Sequence,
    Synonym,
}

fn parse_relation_node(
    node_id: &str,
) -> Option<(MetadataNodeType, String, String, String, &'static str)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?.to_string();
    let schema = parts.next()?.to_string();
    let object_name = parts.next()?.to_string();

    match prefix {
        "table" => Some((MetadataNodeType::Table, database, schema, object_name, "U")),
        "view" => Some((MetadataNodeType::View, database, schema, object_name, "V")),
        _ => None,
    }
}

fn parse_special_node(
    node_id: &str,
) -> Option<(MetadataNodeType, String, String, String, SpecialNodeKind)> {
    let mut parts = node_id.split(':');
    let prefix = parts.next()?;
    let database = parts.next()?.to_string();
    let schema = parts.next()?.to_string();
    let object_name = parts.next()?.to_string();

    match prefix {
        "procedure" => Some((
            MetadataNodeType::Procedure,
            database,
            schema,
            object_name,
            SpecialNodeKind::Procedure,
        )),
        "function" => Some((
            MetadataNodeType::Function,
            database,
            schema,
            object_name,
            SpecialNodeKind::Function,
        )),
        "sequence" => Some((
            MetadataNodeType::Sequence,
            database,
            schema,
            object_name,
            SpecialNodeKind::Sequence,
        )),
        "synonym" => Some((
            MetadataNodeType::Synonym,
            database,
            schema,
            object_name,
            SpecialNodeKind::Synonym,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::metadata::MetadataNodeType;

    use super::{parse_relation_node, parse_special_node};

    #[test]
    fn parse_relation_node_supports_table_and_view() {
        let table = parse_relation_node("table:orders_db:dbo:orders");
        assert!(table.is_some());
        let table = table.expect("table node should parse");
        assert!(matches!(table.0, MetadataNodeType::Table));
        assert_eq!(table.4, "U");

        let view = parse_relation_node("view:orders_db:dbo:orders_view");
        assert!(view.is_some());
        let view = view.expect("view node should parse");
        assert!(matches!(view.0, MetadataNodeType::View));
        assert_eq!(view.4, "V");
    }

    #[test]
    fn parse_special_node_supports_all_sql_server_special_types() {
        let procedure = parse_special_node("procedure:orders_db:dbo:sp_refresh_orders");
        assert!(procedure.is_some());
        assert!(matches!(
            procedure.expect("procedure should parse").0,
            MetadataNodeType::Procedure
        ));

        let function = parse_special_node("function:orders_db:dbo:fn_order_total");
        assert!(function.is_some());
        assert!(matches!(
            function.expect("function should parse").0,
            MetadataNodeType::Function
        ));

        let sequence = parse_special_node("sequence:orders_db:dbo:seq_order_id");
        assert!(sequence.is_some());
        assert!(matches!(
            sequence.expect("sequence should parse").0,
            MetadataNodeType::Sequence
        ));

        let synonym = parse_special_node("synonym:orders_db:dbo:syn_order");
        assert!(synonym.is_some());
        assert!(matches!(
            synonym.expect("synonym should parse").0,
            MetadataNodeType::Synonym
        ));
    }
}
