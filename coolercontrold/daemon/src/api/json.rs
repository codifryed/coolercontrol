// SPDX-FileCopyrightText: 2026 Guy Boldon and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A JSON body whose `OpenAPI` schema follows the direction the data travels.
//!
//! aide documents every type with one schema generator, which describes deserialization.
//! Responses are serialized, so `rename(serialize = ..)` and `skip_serializing_if` made
//! response schemas wrong. Request bodies keep aide's generator; responses use a second one
//! set to serialization. `merge_response_schemas` folds its definitions into the spec.

use aide::generate::GenContext;
use aide::openapi::{
    MediaType, OpenApi, Operation, ParameterSchemaOrContent, ReferenceOr, Response, SchemaObject,
};
use aide::operation::{OperationInput, OperationOutput};
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::response::IntoResponse;
use indexmap::IndexMap;
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Not;

const SCHEMAS_PATH: &str = "#/components/schemas/";
/// Response refs point here until the merge moves them under `SCHEMAS_PATH`.
const SERIALIZE_PATH: &str = "#/x-serialize/";
/// Suffix for the request variant of a type whose two directions differ.
const INPUT_SUFFIX: &str = "Input";

thread_local! {
    // Thread-local like aide's own context, which route declaration fills.
    static SERIALIZE_GENERATOR: RefCell<SchemaGenerator> = RefCell::new(serialize_generator());
}

fn serialize_generator() -> SchemaGenerator {
    SchemaGenerator::new(SchemaSettings::draft07().for_serialize().with(|settings| {
        settings.inline_subschemas = false;
        settings.definitions_path = SERIALIZE_PATH.into();
    }))
}

/// Drop-in for `axum::Json`. Identical on the wire; only the documented schema differs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = JsonRejection;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        #[allow(clippy::disallowed_types)]
        let axum::Json(value) = axum::Json::<T>::from_request(request, state).await?;
        Ok(Self(value))
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> axum::response::Response {
        #[allow(clippy::disallowed_types)]
        axum::Json(self.0).into_response()
    }
}

impl<T: JsonSchema> OperationInput for Json<T> {
    fn operation_input(ctx: &mut GenContext, operation: &mut Operation) {
        #[allow(clippy::disallowed_types)]
        axum::Json::<T>::operation_input(ctx, operation);
    }
}

impl<T: JsonSchema> OperationOutput for Json<T> {
    type Inner = T;

    fn operation_response(_ctx: &mut GenContext, _operation: &mut Operation) -> Option<Response> {
        let (json_schema, description) = serialize_subschema_for::<T>();
        Some(Response {
            description: description.unwrap_or_default(),
            content: IndexMap::from_iter([(
                "application/json".into(),
                MediaType {
                    schema: Some(SchemaObject {
                        json_schema,
                        example: None,
                        external_docs: None,
                    }),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
    }

    // aide's `Json` also lists rejections, but only with `all_error_responses`, which is off.
    fn inferred_responses(
        ctx: &mut GenContext,
        operation: &mut Operation,
    ) -> Vec<(Option<u16>, Response)> {
        Self::operation_response(ctx, operation)
            .map(|response| vec![(Some(200), response)])
            .unwrap_or_default()
    }
}

/// A ref to `T`'s serialize-direction schema, plus that schema's description.
pub fn serialize_subschema_for<T: JsonSchema>() -> (Schema, Option<String>) {
    SERIALIZE_GENERATOR.with_borrow_mut(|generator| {
        let schema = generator.subschema_for::<T>();
        let target = match schema.get("$ref").and_then(Value::as_str) {
            Some(reference) => reference
                .strip_prefix(SERIALIZE_PATH)
                .and_then(|name| generator.definitions().get(name)),
            None => Some(schema.as_value()),
        };
        assert!(
            target.is_some(),
            "a serialize ref resolves to its definition"
        );
        let description = target
            .and_then(|target| target.get("description"))
            .and_then(Value::as_str)
            .map(String::from);
        (schema, description)
    })
}

/// Drops the serialize-direction definitions. For builds that never produce a document.
#[cfg(any(test, not(debug_assertions)))]
pub fn discard_response_schemas() {
    SERIALIZE_GENERATOR.set(serialize_generator());
}

#[cfg(test)]
pub fn response_schema_count() -> usize {
    SERIALIZE_GENERATOR.with_borrow(|generator| generator.definitions().len())
}

/// Completes a document aide has built. Call after `finish_api_with`.
pub fn finish_spec(api: &mut OpenApi) {
    merge_response_schemas(api);
    prune_unreferenced_schemas(api);
}

/// Folds the serialize-direction definitions into `api`, after aide's request-direction
/// definitions.
///
/// A type that is the same in both directions keeps one entry. A type that differs keeps its
/// name for the response, and the request variant gets `INPUT_SUFFIX`.
fn merge_response_schemas(api: &mut OpenApi) {
    let serialized = SERIALIZE_GENERATOR
        .replace(serialize_generator())
        .take_definitions(true);
    let schemas = &mut api.components.get_or_insert_with(Default::default).schemas;
    let shared = shared_names(schemas, &serialized);
    let split: HashSet<String> = serialized
        .keys()
        .filter(|name| schemas.contains_key(*name))
        .filter(|name| shared.contains(*name).not())
        .cloned()
        .collect();
    rename_request_variants(schemas, &split, &serialized);
    for (name, value) in serialized {
        if shared.contains(&name) {
            continue;
        }
        let json_schema = match value {
            Value::Object(object) => Schema::from(object),
            Value::Bool(flag) => Schema::from(flag),
            other => panic!("schemars emitted a non-schema definition for {name}: {other}"),
        };
        let entry = SchemaObject {
            json_schema,
            example: None,
            external_docs: None,
        };
        assert!(schemas.insert(name, entry).is_none());
    }
    schemas.sort_keys();
    // One pass, so a ref is never mapped twice.
    for_each_schema(api, &mut |schema| {
        if let Some(object) = schema.as_object_mut() {
            rewrite_refs(object, &|reference| final_ref(reference, &split));
        }
        assert!(
            referenced_names(schema.as_value(), SERIALIZE_PATH).is_empty(),
            "no ref is left on the serialize path"
        );
    });
}

/// Drops component schemas that nothing in the document refs. aide records a schema for
/// every `Path` and `Query` type, then copies its fields into the parameter list, leaving the
/// definition unused.
fn prune_unreferenced_schemas(api: &mut OpenApi) {
    let mut pending = Vec::new();
    for_each_path_schema(api, &mut |schema| {
        pending.extend(referenced_names(schema.as_value(), SCHEMAS_PATH));
    });
    let Some(components) = api.components.as_mut() else {
        return;
    };
    let schemas = &mut components.schemas;
    // Each name expands once, so this ends within `schemas.len()` expansions.
    let mut reachable = HashSet::with_capacity(schemas.len());
    while let Some(name) = pending.pop() {
        if reachable.contains(&name) {
            continue;
        }
        if let Some(entry) = schemas.get(&name) {
            pending.extend(referenced_names(entry.json_schema.as_value(), SCHEMAS_PATH));
        }
        reachable.insert(name);
    }
    assert!(reachable.len() <= schemas.len());
    schemas.retain(|name, _| reachable.contains(name));
    assert_eq!(
        schemas.len(),
        reachable.len(),
        "every reachable ref resolves"
    );
}

fn rename_request_variants(
    schemas: &mut IndexMap<String, SchemaObject>,
    split: &HashSet<String>,
    serialized: &Map<String, Value>,
) {
    for name in split {
        let renamed = format!("{name}{INPUT_SUFFIX}");
        assert!(
            schemas.contains_key(&renamed).not(),
            "{renamed} already exists"
        );
        assert!(
            serialized.contains_key(&renamed).not(),
            "{renamed} already exists"
        );
        let entry = schemas
            .shift_remove(name)
            .expect("split names are in both maps");
        schemas.insert(renamed, entry);
    }
}

/// Where a ref points once both generators' definitions share `SCHEMAS_PATH`.
fn final_ref(reference: &str, split: &HashSet<String>) -> Option<String> {
    if let Some(name) = reference.strip_prefix(SERIALIZE_PATH) {
        return Some(format!("{SCHEMAS_PATH}{name}"));
    }
    let name = reference.strip_prefix(SCHEMAS_PATH)?;
    split
        .contains(name)
        .then(|| format!("{SCHEMAS_PATH}{name}{INPUT_SUFFIX}"))
}

/// Names whose serialize-direction schema equals the request-direction one, both in
/// itself and in every type it references.
fn shared_names(
    deserialized: &IndexMap<String, SchemaObject>,
    serialized: &Map<String, Value>,
) -> HashSet<String> {
    let to_schemas_path = |reference: &str| {
        Some(format!(
            "{SCHEMAS_PATH}{}",
            reference.strip_prefix(SERIALIZE_PATH)?
        ))
    };
    let mut shared: HashSet<String> = serialized
        .iter()
        .filter(|(name, value)| {
            deserialized.get(*name).is_some_and(|entry| {
                let mut normalized = (*value).clone();
                if let Value::Object(object) = &mut normalized {
                    rewrite_refs(object, &to_schemas_path);
                }
                entry.json_schema.as_value() == &normalized
            })
        })
        .map(|(name, _)| name.clone())
        .collect();
    // Each pass either removes a name or ends, so the bound is never reached.
    for _ in 0..=serialized.len() {
        let differing: Vec<String> = shared
            .iter()
            .filter(|name| {
                referenced_names(&serialized[name.as_str()], SERIALIZE_PATH)
                    .iter()
                    .any(|referenced| shared.contains(referenced).not())
            })
            .cloned()
            .collect();
        if differing.is_empty() {
            for name in &shared {
                assert!(deserialized.contains_key(name));
                assert!(serialized.contains_key(name));
            }
            return shared;
        }
        for name in &differing {
            shared.remove(name);
        }
    }
    unreachable!("the shared set shrinks on every pass");
}

/// Names of the definitions that `value` refs under `prefix`.
fn referenced_names(value: &Value, prefix: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![value];
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(object) => {
                for (key, child) in object {
                    if key == "$ref" {
                        let name = child.as_str().and_then(|r| r.strip_prefix(prefix));
                        names.extend(name.map(String::from));
                    } else {
                        stack.push(child);
                    }
                }
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    names
}

/// Rewrites every `$ref` under `object` that `map` returns a replacement for.
fn rewrite_refs(object: &mut Map<String, Value>, map: &impl Fn(&str) -> Option<String>) {
    let mut stack: Vec<&mut Value> = Vec::new();
    rewrite_object_refs(object, map, &mut stack);
    while let Some(node) = stack.pop() {
        match node {
            Value::Object(child) => rewrite_object_refs(child, map, &mut stack),
            Value::Array(items) => stack.extend(items.iter_mut()),
            _ => {}
        }
    }
}

fn rewrite_object_refs<'a>(
    object: &'a mut Map<String, Value>,
    map: &impl Fn(&str) -> Option<String>,
    stack: &mut Vec<&'a mut Value>,
) {
    for (key, value) in object.iter_mut() {
        if key != "$ref" {
            stack.push(value);
            continue;
        }
        if let Value::String(reference) = value {
            if let Some(replacement) = map(reference) {
                *reference = replacement;
            }
        }
    }
}

/// Visits every schema in the document that can hold a ref: components, parameters,
/// request bodies and response bodies.
fn for_each_schema(api: &mut OpenApi, visit: &mut impl FnMut(&mut Schema)) {
    if let Some(components) = api.components.as_mut() {
        for entry in components.schemas.values_mut() {
            visit(&mut entry.json_schema);
        }
    }
    for_each_path_schema(api, visit);
}

/// Visits the schemas of every operation's parameters, request body and responses.
fn for_each_path_schema(api: &mut OpenApi, visit: &mut impl FnMut(&mut Schema)) {
    let Some(paths) = api.paths.as_mut() else {
        return;
    };
    for item in paths
        .paths
        .values_mut()
        .filter_map(ReferenceOr::as_item_mut)
    {
        let operations = [
            &mut item.get,
            &mut item.put,
            &mut item.post,
            &mut item.delete,
            &mut item.options,
            &mut item.head,
            &mut item.patch,
            &mut item.trace,
        ];
        for operation in operations.into_iter().flatten() {
            for_each_operation_schema(operation, visit);
        }
    }
}

fn for_each_operation_schema(operation: &mut Operation, visit: &mut impl FnMut(&mut Schema)) {
    let parameters = operation.parameters.iter_mut();
    for parameter in parameters.filter_map(ReferenceOr::as_item_mut) {
        match &mut parameter.parameter_data_mut().format {
            ParameterSchemaOrContent::Schema(object) => visit(&mut object.json_schema),
            ParameterSchemaOrContent::Content(content) => for_each_media_schema(content, visit),
        }
    }
    let body = operation.request_body.as_mut();
    if let Some(body) = body.and_then(ReferenceOr::as_item_mut) {
        for_each_media_schema(&mut body.content, visit);
    }
    if let Some(responses) = operation.responses.as_mut() {
        let all = responses
            .default
            .iter_mut()
            .chain(responses.responses.values_mut());
        for response in all.filter_map(ReferenceOr::as_item_mut) {
            for_each_media_schema(&mut response.content, visit);
        }
    }
}

fn for_each_media_schema(
    content: &mut IndexMap<String, MediaType>,
    visit: &mut impl FnMut(&mut Schema),
) {
    for media in content.values_mut() {
        if let Some(object) = media.schema.as_mut() {
            visit(&mut object.json_schema);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aide::axum::routing::{get, post};
    use aide::axum::ApiRouter;
    use serde::Deserialize;

    /// Renamed only when sent, like `DeviceDto::d_type`.
    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Renamed {
        #[serde(rename(serialize = "type"))]
        d_type: u8,
    }

    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Same {
        value: u8,
    }

    /// The same in itself, but holds a type that is not.
    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Holder {
        inner: Renamed,
    }

    #[derive(Deserialize, JsonSchema)]
    struct ItemPath {
        #[allow(dead_code)]
        id: String,
    }

    #[derive(Deserialize, JsonSchema)]
    struct ItemQuery {
        #[allow(dead_code)]
        verbose: bool,
    }

    #[allow(clippy::unused_async)]
    async fn take_params(
        _: axum::extract::Path<ItemPath>,
        _: axum::extract::Query<ItemQuery>,
        Json(_): Json<Holder>,
    ) {
    }

    #[allow(clippy::unused_async)]
    async fn take_renamed(Json(_): Json<Renamed>) {}
    #[allow(clippy::unused_async)]
    async fn give_renamed() -> Json<Renamed> {
        Json(Renamed { d_type: 0 })
    }
    #[allow(clippy::unused_async)]
    async fn take_same(Json(_): Json<Same>) {}
    #[allow(clippy::unused_async)]
    async fn give_same() -> Json<Same> {
        Json(Same { value: 0 })
    }
    #[allow(clippy::unused_async)]
    async fn take_holder(Json(_): Json<Holder>) {}
    #[allow(clippy::unused_async)]
    async fn give_holder() -> Json<Holder> {
        Json(Holder {
            inner: Renamed { d_type: 0 },
        })
    }

    fn spec(router: ApiRouter) -> Value {
        let mut api = OpenApi::default();
        let _ = router.finish_api(&mut api);
        finish_spec(&mut api);
        serde_json::to_value(api).expect("the spec serializes")
    }

    fn request_ref(spec: &Value, path: &str) -> String {
        spec["paths"][path]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"]
            .as_str()
            .expect("the request body is a ref")
            .to_string()
    }

    fn response_ref(spec: &Value, path: &str) -> String {
        spec["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"]
            .as_str()
            .expect("the response body is a ref")
            .to_string()
    }

    /// Goal: a type renamed on the way out documents each direction under its own name.
    #[test]
    fn asymmetric_type_splits_into_response_and_input() {
        let spec = spec(
            ApiRouter::new()
                .api_route("/renamed", post(take_renamed))
                .api_route("/renamed", get(give_renamed)),
        );
        let schemas = &spec["components"]["schemas"];
        assert!(schemas["Renamed"]["properties"].get("type").is_some());
        assert!(schemas["Renamed"]["properties"].get("d_type").is_none());
        assert!(schemas["RenamedInput"]["properties"]
            .get("d_type")
            .is_some());
        assert_eq!(
            request_ref(&spec, "/renamed"),
            "#/components/schemas/RenamedInput"
        );
        assert_eq!(
            response_ref(&spec, "/renamed"),
            "#/components/schemas/Renamed"
        );
    }

    /// Goal: a type that is the same both ways is documented once, under its plain name.
    #[test]
    fn symmetric_type_is_shared() {
        let spec = spec(
            ApiRouter::new()
                .api_route("/same", post(take_same))
                .api_route("/same", get(give_same)),
        );
        let schemas = spec["components"]["schemas"].as_object().expect("schemas");
        assert_eq!(
            schemas.keys().collect::<Vec<_>>(),
            vec!["Same"],
            "only one entry"
        );
        assert_eq!(request_ref(&spec, "/same"), "#/components/schemas/Same");
        assert_eq!(response_ref(&spec, "/same"), "#/components/schemas/Same");
    }

    /// Goal: a type is split when anything it references is split, even if it matches itself.
    #[test]
    fn split_spreads_to_referencing_types() {
        let spec = spec(
            ApiRouter::new()
                .api_route("/holder", post(take_holder))
                .api_route("/holder", get(give_holder)),
        );
        let schemas = &spec["components"]["schemas"];
        assert_eq!(
            schemas["Holder"]["properties"]["inner"]["$ref"],
            "#/components/schemas/Renamed"
        );
        assert_eq!(
            schemas["HolderInput"]["properties"]["inner"]["$ref"],
            "#/components/schemas/RenamedInput"
        );
        assert_eq!(
            request_ref(&spec, "/holder"),
            "#/components/schemas/HolderInput"
        );
        assert_eq!(
            response_ref(&spec, "/holder"),
            "#/components/schemas/Holder"
        );
    }

    /// Goal: a response-only type is documented by what is sent, with no `Input` twin.
    #[test]
    fn response_only_type_uses_serialize_contract() {
        let spec = spec(ApiRouter::new().api_route("/renamed", get(give_renamed)));
        let schemas = spec["components"]["schemas"].as_object().expect("schemas");
        assert!(schemas["Renamed"]["properties"].get("type").is_some());
        assert!(schemas.contains_key("RenamedInput").not());
    }

    /// Goal: path and query types, which aide inlines as parameters, leave no unused schema
    /// behind, while types reached only through another schema survive.
    #[test]
    fn unreferenced_schemas_are_pruned() {
        let spec = spec(ApiRouter::new().api_route("/items/{id}", post(take_params)));
        let schemas = spec["components"]["schemas"].as_object().expect("schemas");
        let mut names: Vec<_> = schemas.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["Holder", "Renamed"]);
        let parameters = spec["paths"]["/items/{id}"]["post"]["parameters"]
            .as_array()
            .expect("the parameters stay inlined");
        assert_eq!(parameters.len(), 2);
    }
}
