//! `Headers`, with the JavaScript object's semantics: names are compared
//! case-insensitively and stored lowercase, `get` joins repeated values with
//! a comma, and a clone shares the same list the way a JS handle does.

use std::cell::RefCell;
use std::rc::Rc;

use http::HeaderMap;
use http::HeaderName;
use http::HeaderValue;

use crate::Error;
use crate::Result;

/// What [`Headers::entries`] yields.
pub type HeaderIterator = std::vec::IntoIter<(String, String)>;

#[derive(Clone, Default)]
pub struct Headers(Rc<RefCell<Vec<(String, String)>>>);

impl std::fmt::Debug for Headers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.0.borrow().iter().map(|(k, v)| (k, v)))
            .finish()
    }
}

impl Headers {
    pub fn new() -> Self {
        Headers(Rc::new(RefCell::new(Vec::new())))
    }

    /// Returns all the values of a header as one comma separated string.
    pub fn get(&self, name: &str) -> Result<Option<String>> {
        let values = self.get_all(name)?;

        Ok((!values.is_empty()).then(|| values.join(", ")))
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    pub fn has(&self, name: &str) -> Result<bool> {
        let name = name.to_ascii_lowercase();

        Ok(self.0.borrow().iter().any(|(key, _)| *key == name))
    }

    /// Adds a value, keeping any already there.
    pub fn append(&self, name: &str, value: &str) -> Result<()> {
        self.0
            .borrow_mut()
            .push((name.to_ascii_lowercase(), value.to_string()));

        Ok(())
    }

    /// Sets a value, replacing any already there.
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        self.delete(name)?;

        self.append(name, value)
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        let name = name.to_ascii_lowercase();

        self.0.borrow_mut().retain(|(key, _)| *key != name);

        Ok(())
    }

    pub fn entries(&self) -> HeaderIterator {
        self.0.borrow().clone().into_iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = String> {
        self.entries().map(|(key, _)| key)
    }

    pub fn values(&self) -> impl Iterator<Item = String> {
        self.entries().map(|(_, value)| value)
    }

    pub fn get_all(&self, name: &str) -> Result<Vec<String>> {
        let name = name.to_ascii_lowercase();

        Ok(self
            .0
            .borrow()
            .iter()
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
            .collect())
    }
}

impl IntoIterator for &Headers {
    type Item = (String, String);
    type IntoIter = HeaderIterator;

    fn into_iter(self) -> Self::IntoIter {
        self.entries()
    }
}

impl<T: AsRef<str>> FromIterator<(T, T)> for Headers {
    fn from_iter<I: IntoIterator<Item = (T, T)>>(iter: I) -> Self {
        let headers = Headers::new();

        for (name, value) in iter {
            let _ = headers.append(name.as_ref(), value.as_ref());
        }

        headers
    }
}

impl<'a, T: AsRef<str>> FromIterator<&'a (T, T)> for Headers {
    fn from_iter<I: IntoIterator<Item = &'a (T, T)>>(iter: I) -> Self {
        let headers = Headers::new();

        for (name, value) in iter {
            let _ = headers.append(name.as_ref(), value.as_ref());
        }

        headers
    }
}

impl From<&HeaderMap> for Headers {
    fn from(map: &HeaderMap) -> Self {
        let headers = Headers::new();

        for (name, value) in map {
            if let Ok(value) = value.to_str() {
                let _ = headers.append(name.as_str(), value);
            }
        }

        headers
    }
}

impl From<HeaderMap> for Headers {
    fn from(map: HeaderMap) -> Self {
        Headers::from(&map)
    }
}

impl From<&Headers> for HeaderMap {
    fn from(headers: &Headers) -> Self {
        let mut map = HeaderMap::new();

        for (name, value) in headers.entries() {
            if let (Ok(name), Ok(value)) =
                (HeaderName::try_from(name), HeaderValue::try_from(value))
            {
                map.append(name, value);
            }
        }

        map
    }
}

impl From<Headers> for HeaderMap {
    fn from(headers: Headers) -> Self {
        HeaderMap::from(&headers)
    }
}

impl TryFrom<&Headers> for crate::wit::wasi::http0_2_0::types::Fields {
    type Error = Error;

    fn try_from(headers: &Headers) -> Result<Self> {
        let entries: Vec<(String, Vec<u8>)> = headers
            .entries()
            .map(|(name, value)| (name, value.into_bytes()))
            .collect();

        crate::wit::wasi::http0_2_0::types::Fields::from_list(&entries)
            .map_err(|e| Error::RustError(format!("invalid headers: {e:?}")))
    }
}
