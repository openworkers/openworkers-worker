//! **Requires** the `d1` feature. SQL through `openworkers:bindings/database`.
//!
//! A statement is prepared and bound guest-side; the host is only called to
//! run it, and answers with the rows as JSON. That is why `D1Result` holds
//! JSON rather than a row cursor.

use std::fmt::Display;
use std::ops::Deref;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::env::EnvBinding;
use crate::wasm_bindgen::JsValue;
use crate::wasm_bindgen::Value;
use crate::wit::openworkers::bindings::database;
use crate::Env;
use crate::Error;
use crate::Result;

/// The largest integer an `f64` holds exactly; past it a bound number is a
/// float whatever it was meant to be.
const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_992.0;

impl Env {
    /// A D1 database binding.
    pub fn d1(&self, binding: &str) -> Result<D1Database> {
        self.get_binding(binding)
    }
}

/// A D1 database.
#[derive(Debug, Clone)]
pub struct D1Database {
    binding: String,
}

impl EnvBinding for D1Database {
    const TYPE_NAME: &'static str = "D1Database";

    fn get(name: &str) -> Result<Self> {
        Ok(D1Database {
            binding: name.to_string(),
        })
    }
}

impl D1Database {
    /// Prepares a statement. Nothing leaves the guest until it is run.
    pub fn prepare<T: Into<String>>(&self, query: T) -> D1PreparedStatement {
        D1PreparedStatement {
            binding: self.binding.clone(),
            query: query.into(),
            params: Vec::new(),
        }
    }

    /// Runs statements one after another. The host has no batch call, so
    /// this is not a transaction.
    pub async fn batch(&self, statements: Vec<D1PreparedStatement>) -> Result<Vec<D1Result>> {
        let mut results = Vec::with_capacity(statements.len());

        for statement in statements {
            results.push(statement.all().await?);
        }

        Ok(results)
    }

    /// Runs one statement with no parameters. The platform validates that a
    /// query is a single statement, so this cannot be a script.
    pub async fn exec(&self, query: &str) -> Result<D1ExecResult> {
        let meta = self.prepare(query).run().await?.meta()?;

        Ok(D1ExecResult {
            count: meta
                .as_ref()
                .and_then(|meta| meta.changes)
                .map(|changes| changes as u32),
            duration: meta.and_then(|meta| meta.duration),
        })
    }

    /// Dumping a database is a Cloudflare API call, not a binding call.
    pub async fn dump(&self) -> Result<Vec<u8>> {
        Err(Error::RustError(
            "d1: dump is not supported by this platform".into(),
        ))
    }
}

/// What [`D1Database::exec`] reports.
#[derive(Debug, Clone, Default)]
pub struct D1ExecResult {
    pub count: Option<u32>,
    pub duration: Option<f64>,
}

/// A prepared statement with its bound parameters.
#[derive(Debug, Clone)]
pub struct D1PreparedStatement {
    binding: String,
    query: String,
    params: Vec<database::SqlParam>,
}

impl D1PreparedStatement {
    /// Binds the positional parameters, in order.
    pub fn bind(mut self, values: &[JsValue]) -> Result<Self> {
        self.params = values.iter().map(to_param).collect::<Result<Vec<_>>>()?;

        Ok(self)
    }

    pub fn bind_refs<'a, T: IntoIterator<Item = &'a U>, U: D1Argument + 'a>(
        mut self,
        values: T,
    ) -> Result<Self> {
        self.params = values
            .into_iter()
            .map(|value| to_param(&value.js_value()))
            .collect::<Result<Vec<_>>>()?;

        Ok(self)
    }

    /// The first row, or one column of it when `col_name` is given.
    pub async fn first<T>(&self, col_name: Option<&str>) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let row = database::first(&self.binding, &self.query, &self.params).map_err(d1_error)?;

        let Some(row) = row else {
            return Ok(None);
        };

        match col_name {
            None => serde_json::from_str(&row)
                .map(Some)
                .map_err(Error::SerdeJsonError),
            Some(column) => {
                let row: serde_json::Value = serde_json::from_str(&row)?;

                match row.get(column) {
                    None => Err(Error::RustError(format!("d1: no column named `{column}`"))),
                    Some(value) => serde_json::from_value(value.clone())
                        .map(Some)
                        .map_err(Error::from),
                }
            }
        }
    }

    /// Runs the statement and keeps only what the host reported about it.
    pub async fn run(&self) -> Result<D1Result> {
        let meta = database::run(&self.binding, &self.query, &self.params).map_err(d1_error)?;

        Ok(D1Result {
            rows: Vec::new(),
            meta,
        })
    }

    /// Runs the statement and collects every row.
    pub async fn all(&self) -> Result<D1Result> {
        let result = database::all(&self.binding, &self.query, &self.params).map_err(d1_error)?;
        let rows: Vec<serde_json::Value> = serde_json::from_str(&result.rows)?;

        Ok(D1Result {
            rows,
            meta: result.meta,
        })
    }

    /// Every row as a list of column values, in the order the query asked
    /// for them.
    pub async fn raw<T>(&self) -> Result<Vec<Vec<T>>>
    where
        T: DeserializeOwned,
    {
        let result = self.all().await?;
        let mut rows = Vec::with_capacity(result.rows.len());

        for row in result.rows {
            let serde_json::Value::Object(columns) = row else {
                return Err(Error::RustError("d1: a row was not an object".into()));
            };

            rows.push(
                columns
                    .into_iter()
                    .map(|(_, value)| serde_json::from_value(value))
                    .collect::<std::result::Result<Vec<T>, _>>()?,
            );
        }

        Ok(rows)
    }
}

/// The rows and the meta of one statement.
#[derive(Debug, Clone)]
pub struct D1Result {
    rows: Vec<serde_json::Value>,
    meta: database::QueryMeta,
}

/// What the host reports about a statement it ran. The fields Cloudflare
/// fills that OpenWorkers has no source for stay `None`.
#[derive(Debug, Clone, Default)]
pub struct D1ResultMeta {
    pub changed_db: Option<bool>,
    pub changes: Option<usize>,
    pub duration: Option<f64>,
    pub last_row_id: Option<i64>,
    pub rows_read: Option<usize>,
    pub rows_written: Option<usize>,
    pub size_after: Option<usize>,
}

impl D1Result {
    /// A statement that failed is reported as an error, so this is always
    /// true for a result the caller is holding.
    pub fn success(&self) -> bool {
        true
    }

    pub fn error(&self) -> Option<String> {
        None
    }

    /// The rows, deserialized.
    pub fn results<T>(&self) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        self.rows
            .iter()
            .map(|row| serde_json::from_value(row.clone()).map_err(Error::SerdeJsonError))
            .collect()
    }

    pub fn meta(&self) -> Result<Option<D1ResultMeta>> {
        Ok(Some(D1ResultMeta {
            changes: Some(self.meta.rows_affected as usize),
            duration: Some(self.meta.duration_ms as f64),
            ..D1ResultMeta::default()
        }))
    }
}

/// A database error, as workers-rs surfaces one.
#[derive(Debug, Clone)]
pub struct D1Error {
    cause: String,
}

impl D1Error {
    pub fn cause(&self) -> String {
        self.cause.clone()
    }
}

impl Display for D1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cause)
    }
}

fn d1_error(cause: String) -> Error {
    Error::D1(D1Error { cause })
}

/// A value bindable as a query parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum D1Type<'a> {
    Null,
    Real(f64),
    Integer(i32),
    Boolean(bool),
    Text(&'a str),
}

impl From<&D1Type<'_>> for JsValue {
    fn from(value: &D1Type) -> Self {
        match value {
            D1Type::Null => JsValue::NULL,
            D1Type::Real(value) => JsValue::from_f64(*value),
            D1Type::Integer(value) => JsValue::from_f64(f64::from(*value)),
            D1Type::Boolean(value) => JsValue::from_bool(*value),
            D1Type::Text(value) => JsValue::from_str(value),
        }
    }
}

/// A [`D1Type`] held by value, so a slice of them can be bound.
#[derive(Debug, Clone)]
pub struct D1PreparedArgument<'a> {
    value: D1Type<'a>,
}

impl<'a> D1PreparedArgument<'a> {
    pub fn new(value: &'a D1Type) -> D1PreparedArgument<'a> {
        D1PreparedArgument {
            value: value.clone(),
        }
    }
}

impl<'a> Deref for D1PreparedArgument<'a> {
    type Target = D1Type<'a>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Anything [`D1PreparedStatement::bind_refs`] accepts.
pub trait D1Argument {
    fn js_value(&self) -> JsValue;
}

impl D1Argument for D1Type<'_> {
    fn js_value(&self) -> JsValue {
        JsValue::from(self)
    }
}

impl D1Argument for D1PreparedArgument<'_> {
    fn js_value(&self) -> JsValue {
        JsValue::from(&self.value)
    }
}

/// Serializes a value the way the `query!` macro binds it.
pub fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue> {
    from_json(serde_json::to_value(value)?)
}

fn from_json(value: serde_json::Value) -> Result<JsValue> {
    Ok(match value {
        serde_json::Value::Null => JsValue::NULL,
        serde_json::Value::Bool(value) => JsValue::from_bool(value),
        serde_json::Value::String(value) => JsValue(Value::Str(value)),
        serde_json::Value::Number(value) => JsValue::from_f64(
            value
                .as_f64()
                .ok_or_else(|| Error::RustError("d1: unbindable number".into()))?,
        ),
        other => JsValue(Value::Str(other.to_string())),
    })
}

fn to_param(value: &JsValue) -> Result<database::SqlParam> {
    Ok(database::SqlParam::Value(to_sql_value(value)?))
}

/// Numbers arrive as `f64` whatever they started as, so a whole number is
/// bound as an integer; SQLite and Postgres both widen it back if needed.
fn to_sql_value(value: &JsValue) -> Result<database::SqlValue> {
    Ok(match &value.0 {
        Value::Null | Value::Undefined => database::SqlValue::Null,
        Value::Bool(value) => database::SqlValue::Boolean(*value),
        Value::Str(value) => database::SqlValue::Text(value.clone()),
        Value::Number(value) => {
            if value.fract() == 0.0 && value.abs() <= MAX_EXACT_INTEGER {
                database::SqlValue::Integer(*value as i64)
            } else {
                database::SqlValue::Float(*value)
            }
        }
        Value::Bytes(_) => {
            return Err(Error::RustError(
                "d1: blob parameters are not supported".into(),
            ))
        }
    })
}

/// Prepares a statement and binds the given parameters.
///
/// ```ignore
/// let query = worker::query!(&db, "select * from t where n > ?1", &min)?;
/// ```
#[macro_export]
macro_rules! query {
    ($db:expr, $query:expr) => {
        $crate::d1::D1Database::prepare($db, $query)
    };
    ($db:expr, $query:expr, $($args:expr),* $(,)?) => {{
        || -> $crate::Result<$crate::d1::D1PreparedStatement> {
            let prepared = $crate::d1::D1Database::prepare($db, $query);
            let bindings = &[$( $crate::d1::to_js_value(&$args)? ),*];

            $crate::d1::D1PreparedStatement::bind(prepared, bindings)
        }()
    }};
}
