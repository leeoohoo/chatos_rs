// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::Bson;

pub fn optional_string_bson(value: Option<String>) -> Bson {
    value.map(Bson::String).unwrap_or(Bson::Null)
}

#[cfg(test)]
mod tests {
    use super::optional_string_bson;

    #[test]
    fn converts_missing_string_to_bson_null() {
        assert_eq!(optional_string_bson(None), mongodb::bson::Bson::Null);
    }
}
