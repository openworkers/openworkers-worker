//! SQL through `openworkers:bindings/database`: `env.DB.query(sql, params)` in
//! a JavaScript worker. The database is PostgreSQL, so parameters are `$1`,
//! `$2` and so on.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::env::EnvBinding;
use crate::wit::openworkers::bindings::database as host;
use crate::Env;
use crate::Error;
use crate::Result;

impl Env {
    /// A database binding, `env.DB` in JavaScript.
    pub fn database(&self, binding: &str) -> Result<BindingDatabase> {
        self.get_binding(binding)
    }
}

#[derive(Debug, Clone)]
pub struct BindingDatabase {
    binding: String,
}

impl EnvBinding for BindingDatabase {
    const TYPE_NAME: &'static str = "BindingDatabase";

    fn get(name: &str) -> Result<Self> {
        Ok(BindingDatabase {
            binding: name.to_string(),
        })
    }
}

impl BindingDatabase {
    /// Runs one statement and returns its rows. `params` bind to `$1`, `$2`
    /// and on, in order; an array binds as a list, for `= ANY($1)`, and an
    /// object as JSON text.
    pub async fn query<T: DeserializeOwned>(&self, sql: &str, params: &[Value]) -> Result<Vec<T>> {
        let params = params.iter().map(to_param).collect::<Result<Vec<_>>>()?;
        let result = host::all(&self.binding, sql, &params)
            .map_err(|cause| Error::RustError(format!("database {}: {cause}", self.binding)))?;

        Ok(serde_json::from_str(&result.rows)?)
    }
}

fn to_param(value: &Value) -> Result<host::SqlParam> {
    match value {
        Value::Array(values) => Ok(host::SqlParam::Values(
            values.iter().map(to_value).collect::<Result<Vec<_>>>()?,
        )),
        value => Ok(host::SqlParam::Value(to_value(value)?)),
    }
}

fn to_value(value: &Value) -> Result<host::SqlValue> {
    Ok(match value {
        Value::Null => host::SqlValue::Null,
        Value::Bool(value) => host::SqlValue::Boolean(*value),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => host::SqlValue::Integer(integer),
            None => host::SqlValue::Float(number.as_f64().expect("a JSON number reads as an f64")),
        },
        Value::String(text) => host::SqlValue::Text(text.clone()),
        Value::Object(_) => host::SqlValue::Text(value.to_string()),
        Value::Array(_) => {
            return Err(Error::RustError(
                "database: a list inside a list parameter does not bind".into(),
            ))
        }
    })
}
