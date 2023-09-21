use crate::fdw::qdrant_fdw::qdrant_client::rows_iterator::RowsIterator;
use crate::fdw::qdrant_fdw::qdrant_client::{QdrantClient, QdrantClientError};
use pgrx::pg_sys::panic::ErrorReport;
use pgrx::{pg_sys, PgBuiltInOids, PgSqlErrorCode};
use std::collections::HashMap;
use supabase_wrappers::interface::{Column, Limit, Qual, Row, Sort};
use supabase_wrappers::prelude::*;
use supabase_wrappers::wrappers_fdw;
use thiserror::Error;

#[wrappers_fdw(
    version = "0.1.0",
    author = "Supabase",
    website = "https://github.com/supabase/wrappers/tree/main/wrappers/src/fdw/qdrant_fdw",
    error_type = "QdrantFdwError"
)]
pub(crate) struct QdrantFdw {
    api_url: String,
    api_key: String,
    rows_iterator: Option<RowsIterator>,
}

impl QdrantFdw {
    fn validate_columns(columns: &[Column]) -> Result<(), QdrantFdwError> {
        let allowed_columns = ["id", "payload", "vector"];
        for column in columns {
            if !allowed_columns.contains(&column.name.as_str()) {
                return Err(QdrantFdwError::QdrantColumnsError(
                    "Only columns named `id`, `payload`, or `vector` are allowed.".to_string(),
                ));
            }

            if column.name == "id" && column.type_oid != PgBuiltInOids::INT8OID.into() {
                return Err(QdrantFdwError::QdrantColumnsError(
                    "Column `id` can only be defined as `bigint`".to_string(),
                ));
            } else if column.name == "payload" && column.type_oid != PgBuiltInOids::JSONBOID.into()
            {
                return Err(QdrantFdwError::QdrantColumnsError(
                    "Column `payload` can only be defined as `jsonb`".to_string(),
                ));
            } else if column.name == "vector"
                && column.type_oid != PgBuiltInOids::FLOAT4ARRAYOID.into()
            {
                return Err(QdrantFdwError::QdrantColumnsError(
                    "Column `vector` can only be defined as `real[]`".to_string(),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
enum QdrantFdwError {
    #[error("{0}")]
    OptionsError(#[from] OptionsError),

    #[error("{0}")]
    QdrantClientError(#[from] QdrantClientError),

    #[error("{0}")]
    QdrantColumnsError(String),
}

impl From<QdrantFdwError> for ErrorReport {
    fn from(value: QdrantFdwError) -> Self {
        match value {
            QdrantFdwError::OptionsError(e) => e.into(),
            QdrantFdwError::QdrantClientError(e) => e.into(),
            QdrantFdwError::QdrantColumnsError(_) => {
                ErrorReport::new(PgSqlErrorCode::ERRCODE_FDW_ERROR, format!("{value}"), "")
            }
        }
    }
}

impl ForeignDataWrapper<QdrantFdwError> for QdrantFdw {
    fn new(options: &HashMap<String, String>) -> Result<Self, QdrantFdwError>
    where
        Self: Sized,
    {
        let api_url = require_option("api_url", options)?.to_string();
        let api_key = require_option("api_key", options)?.to_string();
        Ok(Self {
            api_url,
            api_key,
            rows_iterator: None,
        })
    }

    fn begin_scan(
        &mut self,
        _quals: &[Qual],
        columns: &[Column],
        _sorts: &[Sort],
        _limit: &Option<Limit>,
        options: &HashMap<String, String>,
    ) -> Result<(), QdrantFdwError> {
        Self::validate_columns(columns)?;
        let collection_name = require_option("collection_name", options)?;

        let qdrant_client = QdrantClient::new(&self.api_url, &self.api_key)?;
        self.rows_iterator = Some(RowsIterator::new(
            collection_name.to_string(),
            columns.to_vec(),
            1000,
            qdrant_client,
        ));
        Ok(())
    }

    fn iter_scan(&mut self, row: &mut Row) -> Result<Option<()>, QdrantFdwError> {
        let rows_iterator = self
            .rows_iterator
            .as_mut()
            .expect("Can't be None as rows_iterator is initialized in begin_scan");
        if let Some(new_row_result) = rows_iterator.next() {
            let new_row = new_row_result?;
            row.replace_with(new_row);
            Ok(Some(()))
        } else {
            Ok(None)
        }
    }

    fn end_scan(&mut self) -> Result<(), QdrantFdwError> {
        Ok(())
    }

    fn validator(
        options: Vec<Option<String>>,
        catalog: Option<pg_sys::Oid>,
    ) -> Result<(), QdrantFdwError> {
        if let Some(oid) = catalog {
            if oid == FOREIGN_SERVER_RELATION_ID {
                check_options_contain(&options, "api_url")?;
                check_options_contain(&options, "api_key")?;
            } else if oid == FOREIGN_TABLE_RELATION_ID {
                check_options_contain(&options, "collection_name")?;
            }
        }

        Ok(())
    }
}
