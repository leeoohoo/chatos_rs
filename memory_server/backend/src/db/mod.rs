mod connection;
mod index_helpers;
mod normalize;
mod schema;

use mongodb::Database;

pub type Db = Database;

pub use self::connection::init_pool;
pub use self::schema::init_schema;
