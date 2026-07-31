// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum NumberFormat {
    Integer,
    Decimal2,
    Percent2,
    Date,
    DateTime,
}

impl NumberFormat {
    pub(super) fn parse(value: &str) -> Result<Option<Self>> {
        match value {
            "general" => Ok(None),
            "integer" => Ok(Some(Self::Integer)),
            "decimal_2" => Ok(Some(Self::Decimal2)),
            "percent_2" => Ok(Some(Self::Percent2)),
            "date" => Ok(Some(Self::Date)),
            "datetime" => Ok(Some(Self::DateTime)),
            _ => Err(anyhow!("unsupported XLSX number_format: {value}")),
        }
    }

    pub(super) fn built_in_id(self) -> u32 {
        match self {
            Self::Integer => 1,
            Self::Decimal2 => 2,
            Self::Percent2 => 10,
            Self::Date => 14,
            Self::DateTime => 22,
        }
    }

    pub(super) fn generated_style_id(self) -> u32 {
        match self {
            Self::Integer => 1,
            Self::Decimal2 => 2,
            Self::Percent2 => 3,
            Self::Date => 4,
            Self::DateTime => 5,
        }
    }

    pub(super) fn all() -> [Self; 5] {
        [
            Self::Integer,
            Self::Decimal2,
            Self::Percent2,
            Self::Date,
            Self::DateTime,
        ]
    }
}

#[derive(Clone, Debug)]
pub(super) enum PrimitiveCellValue {
    Blank,
    Bool(bool),
    Number(String),
    Text(String),
}

#[derive(Clone, Debug)]
pub(super) enum CellValue {
    Primitive(PrimitiveCellValue),
    Formula {
        expression: String,
        cached_value: Option<PrimitiveCellValue>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct CellInput {
    pub(super) value: CellValue,
    pub(super) number_format: Option<NumberFormat>,
}

#[derive(Debug)]
pub(super) struct WorksheetInput {
    pub(super) name: String,
    pub(super) rows: Vec<Vec<CellInput>>,
    pub(super) freeze_rows: u32,
    pub(super) column_widths: BTreeMap<u16, f64>,
}
