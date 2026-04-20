use crate::{
    domain::{meta::DbType, meta::DbTypeDescriptor},
    drivers::{
        mock::build_mock_drivers, mongodb::MongoDbDriver, mysql::MySqlDriver, oracle::OracleDriver,
        postgres::PostgresDriver, sqlite::SqliteDriver, sqlserver::SqlServerDriver,
        traits::DatabaseDriver,
    },
};
use std::{collections::HashMap, sync::Arc};

pub struct DriverRegistry {
    drivers: HashMap<DbType, Arc<dyn DatabaseDriver>>,
}

impl DriverRegistry {
    pub fn with_default_drivers() -> Self {
        let mut drivers = HashMap::new();
        for driver in build_mock_drivers() {
            drivers.insert(driver.db_type(), driver);
        }
        drivers.insert(DbType::Postgres, Arc::new(PostgresDriver::new()));
        drivers.insert(DbType::MySql, Arc::new(MySqlDriver::new()));
        drivers.insert(DbType::Sqlite, Arc::new(SqliteDriver::new()));
        drivers.insert(DbType::SqlServer, Arc::new(SqlServerDriver::new()));
        drivers.insert(DbType::MongoDb, Arc::new(MongoDbDriver::new()));
        drivers.insert(DbType::Oracle, Arc::new(OracleDriver::new()));

        Self { drivers }
    }

    pub fn get(&self, db_type: &DbType) -> Option<Arc<dyn DatabaseDriver>> {
        self.drivers.get(db_type).cloned()
    }

    pub fn descriptors(&self) -> Vec<DbTypeDescriptor> {
        let mut items: Vec<DbTypeDescriptor> = self
            .drivers
            .values()
            .map(|driver| driver.descriptor())
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items
    }
}
