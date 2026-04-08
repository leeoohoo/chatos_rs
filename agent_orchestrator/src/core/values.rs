use mongodb::bson::Bson;

pub fn optional_string_bson(value: Option<String>) -> Bson {
    value.map(Bson::String).unwrap_or(Bson::Null)
}

pub fn bool_to_sqlite_int(value: bool) -> i32 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{bool_to_sqlite_int, optional_string_bson};

    #[test]
    fn converts_missing_string_to_bson_null() {
        assert_eq!(optional_string_bson(None), mongodb::bson::Bson::Null);
    }

    #[test]
    fn converts_bool_to_sqlite_integer() {
        assert_eq!(bool_to_sqlite_int(true), 1);
        assert_eq!(bool_to_sqlite_int(false), 0);
    }
}
