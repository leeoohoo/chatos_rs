use serde_json::{json, Value};

pub(super) fn kv_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "message": { "type": "string" },
            "fields": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "minLength": 1 },
                        "name": { "type": "string", "minLength": 1 },
                        "id": { "type": "string", "minLength": 1 },
                        "label": { "type": "string" },
                        "description": { "type": "string" },
                        "placeholder": { "type": "string" },
                        "default": { "type": "string" },
                        "required": { "type": "boolean" },
                        "multiline": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "additionalProperties": false
                }
            },
            "allow_cancel": { "type": "boolean" },
        },
        "required": ["fields"],
        "additionalProperties": false
    })
}

pub(super) fn choice_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "message": { "type": "string" },
            "multiple": { "type": "boolean" },
            "options": {
                "type": "array",
                "minItems": 1,
                "maxItems": 60,
                "items": {
                    "type": "object",
                    "properties": {
                        "value": { "type": "string", "minLength": 1 },
                        "label": { "type": "string" },
                        "description": { "type": "string" }
                    },
                    "required": ["value"],
                    "additionalProperties": false
                }
            },
            "default": {
                "type": "string"
            },
            "min_selections": { "type": "integer", "minimum": 0, "maximum": 60 },
            "max_selections": { "type": "integer", "minimum": 1, "maximum": 60 },
            "allow_cancel": { "type": "boolean" },
        },
        "required": ["options"],
        "additionalProperties": false
    })
}

pub(super) fn mixed_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "message": { "type": "string" },
            "fields": {
                "type": "array",
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "key": { "type": "string", "minLength": 1 },
                        "name": { "type": "string", "minLength": 1 },
                        "id": { "type": "string", "minLength": 1 },
                        "label": { "type": "string" },
                        "description": { "type": "string" },
                        "placeholder": { "type": "string" },
                        "default": { "type": "string" },
                        "required": { "type": "boolean" },
                        "multiline": { "type": "boolean" },
                        "secret": { "type": "boolean" }
                    },
                    "additionalProperties": false
                }
            },
            "choice": {
                "type": "object",
                "properties": {
                    "multiple": { "type": "boolean" },
                    "options": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 60,
                        "items": {
                            "type": "object",
                            "properties": {
                                "value": { "type": "string", "minLength": 1 },
                                "label": { "type": "string" },
                                "description": { "type": "string" }
                            },
                            "required": ["value"],
                            "additionalProperties": false
                        }
                    },
                    "default": {
                        "type": "string"
                    },
                    "min_selections": { "type": "integer", "minimum": 0, "maximum": 60 },
                    "max_selections": { "type": "integer", "minimum": 1, "maximum": 60 }
                },
                "required": ["options"],
                "additionalProperties": false
            },
            "allow_cancel": { "type": "boolean" },
        },
        "additionalProperties": false
    })
}
