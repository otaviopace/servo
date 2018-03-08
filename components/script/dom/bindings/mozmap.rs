/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The `MozMap` (open-ended dictionary) type.

use crate::dom::bindings::conversions::jsid_to_string;
use crate::dom::bindings::error::report_pending_exception;
use crate::dom::bindings::str::DOMString;
use indexmap::IndexMap;
use js::conversions::{ConversionResult, FromJSValConvertible, ToJSValConvertible};
use js::jsapi::JSContext;
use js::jsapi::JS_NewPlainObject;
use js::jsapi::JSITER_HIDDEN;
use js::jsapi::JSITER_OWNONLY;
use js::jsapi::JSITER_SYMBOLS;
use js::jsapi::JSPROP_ENUMERATE;
use js::jsval::ObjectValue;
use js::jsval::UndefinedValue;
use js::rust::wrappers::GetPropertyKeys;
use js::rust::wrappers::JS_DefineUCProperty2;
use js::rust::wrappers::JS_GetPropertyById;
use js::rust::HandleId;
use js::rust::HandleValue;
use js::rust::IdVector;
use js::rust::MutableHandleValue;
use std::cmp::Eq;
use std::hash::Hash;
use std::marker::Sized;
use std::ops::Deref;

pub trait MozMapKey : Eq + Hash + Sized {
    fn to_utf16_vec(&self) -> Vec<u16>;
    unsafe fn from_id(cx: *mut JSContext, id: HandleId) -> Option<Self>;
}

impl MozMapKey for DOMString {
    fn to_utf16_vec(&self) -> Vec<u16> {
        self.encode_utf16().collect::<Vec<_>>()
    }

    unsafe fn from_id(cx: *mut JSContext, id: HandleId) -> Option<DOMString> {
        jsid_to_string(cx, id)
    }
}

/// The `MozMap` (open-ended dictionary) type.
#[derive(Clone, JSTraceable)]
pub struct MozMap<K: MozMapKey, V> {
    map: IndexMap<K, V>,
}

impl<K: MozMapKey, V> MozMap<K, V> {
    /// Create an empty `MozMap`.
    pub fn new() -> Self {
        MozMap {
            map: IndexMap::new(),
        }
    }
}

impl<K: MozMapKey, V> Deref for MozMap<K, V> {
    type Target = IndexMap<K, V>;

    fn deref(&self) -> &IndexMap<K, V> {
        &self.map
    }
}

impl<K, V, C> FromJSValConvertible for MozMap<K, V>
where
    K: MozMapKey,
    V: FromJSValConvertible<Config = C>,
    C: Clone,
{
    type Config = C;
    unsafe fn from_jsval(
        cx: *mut JSContext,
        value: HandleValue,
        config: C,
    ) -> Result<ConversionResult<Self>, ()> {
        if !value.is_object() {
            return Ok(ConversionResult::Failure(
                "MozMap value was not an object".into(),
            ));
        }

        rooted!(in(cx) let object = value.to_object());
        let ids = IdVector::new(cx);
        if !GetPropertyKeys(
            cx,
            object.handle(),
            JSITER_OWNONLY | JSITER_HIDDEN | JSITER_SYMBOLS,
            ids.get(),
        ) {
            // TODO: can GetPropertyKeys fail?
            // (it does so if the object has duplicate keys)
            // https://github.com/servo/servo/issues/21462
            report_pending_exception(cx, false);
            return Ok(ConversionResult::Failure(
                "Getting MozMap value property keys failed".into(),
            ));
        }

        let mut map = IndexMap::new();
        for id in &*ids {
            rooted!(in(cx) let id = *id);

            rooted!(in(cx) let mut property = UndefinedValue());
            if !JS_GetPropertyById(cx, object.handle(), id.handle(), property.handle_mut()) {
                return Err(());
            }

            let property = match V::from_jsval(cx, property.handle(), config.clone())? {
                ConversionResult::Success(property) => property,
                ConversionResult::Failure(message) => {
                    return Ok(ConversionResult::Failure(message))
                },
            };

            // TODO: Is this guaranteed to succeed?
            // https://github.com/servo/servo/issues/21463
            if let Some(key) = K::from_id(cx, id.handle()) {
                map.insert(key, property);
            }
        }

        Ok(ConversionResult::Success(MozMap { map: map }))
    }
}

impl<K, V> ToJSValConvertible for MozMap<K, V>
    where K: MozMapKey,
          V: ToJSValConvertible  {
    #[inline]
    unsafe fn to_jsval(&self, cx: *mut JSContext, mut rval: MutableHandleValue) {
        rooted!(in(cx) let js_object = JS_NewPlainObject(cx));
        assert!(!js_object.handle().is_null());

        rooted!(in(cx) let mut js_value = UndefinedValue());
        for (key, value) in &self.map {
            let key = key.to_utf16_vec();
            value.to_jsval(cx, js_value.handle_mut());

            assert!(JS_DefineUCProperty2(
                cx,
                js_object.handle(),
                key.as_ptr(),
                key.len(),
                js_value.handle(),
                JSPROP_ENUMERATE as u32
            ));
        }

        rval.set(ObjectValue(js_object.handle().get()));
    }
}
