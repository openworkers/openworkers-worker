//! Stand-ins for the two JavaScript crates workers-rs re-exports.
//!
//! A workers-rs application reaches through `worker::wasm_bindgen::JsValue`
//! to bind D1 parameters and through `worker::js_sys::Date` for the clock.
//! There is no JavaScript here, so `JsValue` is a plain tagged value and
//! `Date` reads `wasi:clocks/wall-clock`. Everything else those crates
//! offer is absent.

/// See [`wasm_bindgen::JsValue`].
pub mod wasm_bindgen {
    /// The subset of a JavaScript value the bindings can carry.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Value {
        Null,
        Undefined,
        Bool(bool),
        Number(f64),
        Str(String),
        Bytes(Vec<u8>),
    }

    /// A value crossing the host boundary, shaped like `wasm_bindgen::JsValue`
    /// so that call sites binding query parameters do not have to change.
    #[derive(Clone, Debug, PartialEq)]
    pub struct JsValue(pub Value);

    impl JsValue {
        pub const NULL: JsValue = JsValue(Value::Null);
        pub const UNDEFINED: JsValue = JsValue(Value::Undefined);
        pub const TRUE: JsValue = JsValue(Value::Bool(true));
        pub const FALSE: JsValue = JsValue(Value::Bool(false));

        // Named after wasm_bindgen's inherent constructor, not `FromStr`
        #[allow(clippy::should_implement_trait)]
        pub fn from_str(value: &str) -> Self {
            JsValue(Value::Str(value.to_string()))
        }

        pub fn from_f64(value: f64) -> Self {
            JsValue(Value::Number(value))
        }

        pub fn from_bool(value: bool) -> Self {
            JsValue(Value::Bool(value))
        }

        pub fn as_string(&self) -> Option<String> {
            match &self.0 {
                Value::Str(value) => Some(value.clone()),
                _ => None,
            }
        }

        pub fn as_f64(&self) -> Option<f64> {
            match self.0 {
                Value::Number(value) => Some(value),
                _ => None,
            }
        }

        pub fn as_bool(&self) -> Option<bool> {
            match self.0 {
                Value::Bool(value) => Some(value),
                _ => None,
            }
        }

        pub fn is_null(&self) -> bool {
            matches!(self.0, Value::Null)
        }

        pub fn is_undefined(&self) -> bool {
            matches!(self.0, Value::Undefined)
        }

        pub fn is_string(&self) -> bool {
            matches!(self.0, Value::Str(_))
        }
    }

    impl std::fmt::Display for JsValue {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match &self.0 {
                Value::Null => write!(f, "null"),
                Value::Undefined => write!(f, "undefined"),
                Value::Bool(value) => write!(f, "{value}"),
                Value::Number(value) => write!(f, "{value}"),
                Value::Str(value) => write!(f, "{value}"),
                Value::Bytes(value) => write!(f, "<{} bytes>", value.len()),
            }
        }
    }

    impl From<&str> for JsValue {
        fn from(value: &str) -> Self {
            JsValue::from_str(value)
        }
    }

    impl From<String> for JsValue {
        fn from(value: String) -> Self {
            JsValue(Value::Str(value))
        }
    }

    impl From<bool> for JsValue {
        fn from(value: bool) -> Self {
            JsValue(Value::Bool(value))
        }
    }

    impl From<Vec<u8>> for JsValue {
        fn from(value: Vec<u8>) -> Self {
            JsValue(Value::Bytes(value))
        }
    }

    macro_rules! from_number {
        ($($ty:ty),*) => {
            $(
                impl From<$ty> for JsValue {
                    fn from(value: $ty) -> Self {
                        JsValue(Value::Number(value as f64))
                    }
                }
            )*
        };
    }

    from_number!(f32, f64, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

    impl<T: Into<JsValue>> From<Option<T>> for JsValue {
        fn from(value: Option<T>) -> Self {
            value.map_or(JsValue::NULL, Into::into)
        }
    }
}

/// See [`js_sys::Date`].
pub mod js_sys {
    use crate::wit::wasi::clocks::wall_clock;

    /// The slice of the JavaScript `Date` a worker actually reads.
    #[derive(Debug)]
    pub struct Date;

    impl Date {
        /// Milliseconds since the Unix epoch.
        pub fn now() -> f64 {
            let now = wall_clock::now();

            now.seconds as f64 * 1_000.0 + f64::from(now.nanoseconds) / 1_000_000.0
        }
    }
}

/// See [`wasm_bindgen_futures::spawn_local`].
pub mod wasm_bindgen_futures {
    pub use crate::rt::spawn_local;
}
