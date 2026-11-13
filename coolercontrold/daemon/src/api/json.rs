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
    Callback, Header, MediaType, OpenApi, Operation, Parameter, ParameterSchemaOrContent, PathItem,
    ReferenceOr, RequestBody, Response, SchemaObject,
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
/// Suffixes for the two variants of a type whose directions differ. aide's pending fix
/// (tamasfe/aide#303) and `FastAPI` name them the same way.
const INPUT_SUFFIX: &str = "-Input";
const OUTPUT_SUFFIX: &str = "-Output";

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
        #[expect(
            clippy::disallowed_types,
            reason = "the wrapper delegates to axum::Json"
        )]
        let axum::Json(value) = axum::Json::<T>::from_request(request, state).await?;
        Ok(Self(value))
    }
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> axum::response::Response {
        axum::Json(self.0).into_response()
    }
}

impl<T: JsonSchema> OperationInput for Json<T> {
    fn operation_input(ctx: &mut GenContext, operation: &mut Operation) {
        #[expect(
            clippy::disallowed_types,
            reason = "the wrapper delegates to axum::Json"
        )]
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
    for_each_schema(api, &mut |schema| {
        if let Some(object) = schema.as_object_mut() {
            restore_prefix_items(object);
        }
    });
}

/// Folds the serialize-direction definitions into `api`, after aide's request-direction
/// definitions.
///
/// A type that is the same in both directions keeps one entry under its name. A type that
/// differs splits into `INPUT_SUFFIX` and `OUTPUT_SUFFIX` variants.
fn merge_response_schemas(api: &mut OpenApi) {
    let serialized = SERIALIZE_GENERATOR
        .replace(serialize_generator())
        .take_definitions(true);
    let schemas = &mut api.components.get_or_insert_with(Default::default).schemas;
    assert_unnumbered_names(schemas.keys());
    assert_unnumbered_names(serialized.keys());
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
        let name = output_name(name, &split);
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
    for name in &split {
        assert!(
            schemas.contains_key(name).not(),
            "{name} is split, not kept"
        );
    }
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
    for_each_root_schema(api, &mut |schema| {
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

/// Keywords whose values are instance data, never subschemas.
const DATA_KEYWORDS: [&str; 5] = ["const", "default", "enum", "example", "examples"];
/// Keywords whose values map names to subschemas, so their keys are never keywords.
const NAMED_SCHEMA_KEYWORDS: [&str; 5] = [
    "properties",
    "patternProperties",
    "dependentSchemas",
    "$defs",
    "definitions",
];

/// Writes tuples back as `prefixItems`. Both generators use schemars' draft-07 settings, whose
/// `ReplacePrefixItems` transform emits a tuple as `items: [..]`. `OpenAPI` 3.1 schemas are
/// JSON Schema 2020-12, where an `items` array is invalid.
fn restore_prefix_items(root: &mut Map<String, Value>) {
    let mut stack = vec![root];
    // Each node is popped once and pushes only its own children, so this ends.
    while let Some(object) = stack.pop() {
        if object.get("items").is_some_and(Value::is_array) {
            assert!(object.contains_key("prefixItems").not());
            if let Some(tuple) = object.remove("items") {
                object.insert("prefixItems".into(), tuple);
            }
            // draft-07 holds the schema for items past the tuple here.
            if let Some(rest) = object.remove("additionalItems") {
                object.insert("items".into(), rest);
            }
        }
        assert!(object.contains_key("additionalItems").not());
        for (keyword, value) in object.iter_mut() {
            if DATA_KEYWORDS.contains(&keyword.as_str()) {
                continue;
            }
            let names_subschemas = NAMED_SCHEMA_KEYWORDS.contains(&keyword.as_str());
            match value {
                Value::Object(map) => {
                    if names_subschemas {
                        stack.extend(map.values_mut().filter_map(Value::as_object_mut));
                    } else {
                        stack.push(map);
                    }
                }
                Value::Array(subschemas) => {
                    stack.extend(subschemas.iter_mut().filter_map(Value::as_object_mut));
                }
                _ => {}
            }
        }
    }
}

/// Fails on a name schemars numbered because two types share a base name. Each generator
/// numbers in the order it meets the types, so `Foo2` can mean a different type in each map,
/// and the number shifts whenever routes are reordered, renaming a client type.
fn assert_unnumbered_names<'a>(names: impl Iterator<Item = &'a String>) {
    let names: HashSet<&str> = names.map(String::as_str).collect();
    for name in &names {
        let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        for split in (name.len() - digits)..name.len() {
            let base = &name[..split];
            assert!(
                names.contains(base).not(),
                "{name} collides with {base}; give one type a distinct #[schemars(rename)]"
            );
        }
    }
}

fn rename_request_variants(
    schemas: &mut IndexMap<String, SchemaObject>,
    split: &HashSet<String>,
    serialized: &Map<String, Value>,
) {
    for name in split {
        let renamed = format!("{name}{INPUT_SUFFIX}");
        for taken in [&renamed, &format!("{name}{OUTPUT_SUFFIX}")] {
            assert!(schemas.contains_key(taken).not(), "{taken} already exists");
            assert!(
                serialized.contains_key(taken).not(),
                "{taken} already exists"
            );
        }
        let entry = schemas
            .shift_remove(name)
            .expect("split names are in both maps");
        schemas.insert(renamed, entry);
    }
}

/// The name a serialize-direction definition takes in the merged document.
fn output_name(name: String, split: &HashSet<String>) -> String {
    if split.contains(&name) {
        format!("{name}{OUTPUT_SUFFIX}")
    } else {
        name
    }
}

/// Where a ref points once both generators' definitions share `SCHEMAS_PATH`.
fn final_ref(reference: &str, split: &HashSet<String>) -> Option<String> {
    if let Some(name) = reference.strip_prefix(SERIALIZE_PATH) {
        let name = output_name(name.to_string(), split);
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
                        let name = child
                            .as_str()
                            .and_then(|reference| reference.strip_prefix(prefix));
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
        if key == "$ref" {
            if let Value::String(reference) = value {
                if let Some(replacement) = map(reference) {
                    *reference = replacement;
                }
            }
        } else {
            stack.push(value);
        }
    }
}

/// Visits every schema in the document.
fn for_each_schema(api: &mut OpenApi, visit: &mut impl FnMut(&mut Schema)) {
    if let Some(components) = api.components.as_mut() {
        for entry in components.schemas.values_mut() {
            visit(&mut entry.json_schema);
        }
    }
    for_each_root_schema(api, visit);
}

/// A part of the document that holds schemas, directly or through other parts.
enum Node<'a> {
    PathItem(&'a mut PathItem),
    Operation(&'a mut Operation),
    Callback(&'a mut Callback),
    Parameter(&'a mut Parameter),
    RequestBody(&'a mut RequestBody),
    Response(&'a mut Response),
    Header(&'a mut Header),
    Format(&'a mut ParameterSchemaOrContent),
    Media(&'a mut MediaType),
}

/// Visits every schema outside `components.schemas`. A component schema is in use when one
/// of these refs it, directly or through other component schemas.
fn for_each_root_schema(api: &mut OpenApi, visit: &mut impl FnMut(&mut Schema)) {
    let mut stack = Vec::new();
    if let Some(paths) = api.paths.as_mut() {
        stack.extend(items(paths.paths.values_mut()).map(Node::PathItem));
    }
    stack.extend(items(api.webhooks.values_mut()).map(Node::PathItem));
    if let Some(components) = api.components.as_mut() {
        stack.extend(items(components.path_items.values_mut()).map(Node::PathItem));
        stack.extend(items(components.callbacks.values_mut()).map(Node::Callback));
        stack.extend(items(components.parameters.values_mut()).map(Node::Parameter));
        stack.extend(items(components.request_bodies.values_mut()).map(Node::RequestBody));
        stack.extend(items(components.responses.values_mut()).map(Node::Response));
        stack.extend(items(components.headers.values_mut()).map(Node::Header));
    }
    // Each node is popped once and pushes only its own children, so this ends.
    while let Some(node) = stack.pop() {
        expand(node, &mut stack, visit);
    }
}

/// Visits the schemas `node` holds directly and pushes the parts that hold more.
fn expand<'a>(node: Node<'a>, stack: &mut Vec<Node<'a>>, visit: &mut impl FnMut(&mut Schema)) {
    match node {
        Node::PathItem(item) => {
            stack.extend(items(item.parameters.iter_mut()).map(Node::Parameter));
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
            stack.extend(operations.into_iter().flatten().map(Node::Operation));
        }
        Node::Operation(operation) => {
            stack.extend(items(operation.parameters.iter_mut()).map(Node::Parameter));
            stack.extend(items(operation.request_body.as_mut()).map(Node::RequestBody));
            if let Some(responses) = operation.responses.as_mut() {
                let all = responses
                    .default
                    .iter_mut()
                    .chain(responses.responses.values_mut());
                stack.extend(items(all).map(Node::Response));
            }
            stack.extend(items(operation.callbacks.values_mut()).map(Node::Callback));
        }
        Node::Callback(callback) => {
            stack.extend(items(callback.values_mut()).map(Node::PathItem));
        }
        Node::Parameter(parameter) => {
            stack.push(Node::Format(&mut parameter.parameter_data_mut().format));
        }
        Node::Header(header) => stack.push(Node::Format(&mut header.format)),
        Node::Format(ParameterSchemaOrContent::Schema(object)) => {
            visit(&mut object.json_schema);
        }
        Node::Format(ParameterSchemaOrContent::Content(content)) => {
            stack.extend(content.values_mut().map(Node::Media));
        }
        Node::RequestBody(body) => stack.extend(body.content.values_mut().map(Node::Media)),
        Node::Response(response) => {
            stack.extend(items(response.headers.values_mut()).map(Node::Header));
            stack.extend(response.content.values_mut().map(Node::Media));
        }
        Node::Media(media) => {
            if let Some(object) = media.schema.as_mut() {
                visit(&mut object.json_schema);
            }
            for encoding in media.encoding.values_mut() {
                stack.extend(items(encoding.headers.values_mut()).map(Node::Header));
            }
        }
    }
}

/// The inline items among `refs`. A `$ref` to a non-schema component is reached through
/// that component instead.
fn items<'a, T: 'a>(
    refs: impl IntoIterator<Item = &'a mut ReferenceOr<T>>,
) -> impl Iterator<Item = &'a mut T> {
    refs.into_iter().filter_map(ReferenceOr::as_item_mut)
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
        #[expect(dead_code, reason = "only the schema is read")]
        id: String,
    }

    #[derive(Deserialize, JsonSchema)]
    struct ItemQuery {
        #[expect(dead_code, reason = "only the schema is read")]
        verbose: bool,
    }

    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_params(
        _: axum::extract::Path<ItemPath>,
        _: axum::extract::Query<ItemQuery>,
        Json(_): Json<Holder>,
    ) {
    }

    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_renamed(Json(_): Json<Renamed>) {}
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn give_renamed() -> Json<Renamed> {
        Json(Renamed { d_type: 0 })
    }
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_same(Json(_): Json<Same>) {}
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn give_same() -> Json<Same> {
        Json(Same { value: 0 })
    }
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_holder(Json(_): Json<Holder>) {}
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
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
    /// Method: route the type as both a request and a response body, then check each body's
    /// ref and the field names of the two schemas.
    #[test]
    fn asymmetric_type_splits_into_input_and_output() {
        let spec = spec(
            ApiRouter::new()
                .api_route("/renamed", post(take_renamed))
                .api_route("/renamed", get(give_renamed)),
        );
        let schemas = &spec["components"]["schemas"];
        assert!(schemas["Renamed-Output"]["properties"]
            .get("type")
            .is_some());
        assert!(schemas["Renamed-Output"]["properties"]
            .get("d_type")
            .is_none());
        assert!(schemas.get("Renamed").is_none());
        assert!(schemas["Renamed-Input"]["properties"]
            .get("d_type")
            .is_some());
        assert_eq!(
            request_ref(&spec, "/renamed"),
            "#/components/schemas/Renamed-Input"
        );
        assert_eq!(
            response_ref(&spec, "/renamed"),
            "#/components/schemas/Renamed-Output"
        );
    }

    /// Goal: a type that is the same both ways is documented once, under its plain name.
    /// Method: route the type both ways, then check the schema list and both body refs.
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
    /// Method: route a holder of a split type both ways, then check that each holder variant
    /// refs the matching variant of the inner type.
    #[test]
    fn split_spreads_to_referencing_types() {
        let spec = spec(
            ApiRouter::new()
                .api_route("/holder", post(take_holder))
                .api_route("/holder", get(give_holder)),
        );
        let schemas = &spec["components"]["schemas"];
        assert!(schemas.get("Holder").is_none());
        assert_eq!(
            schemas["Holder-Output"]["properties"]["inner"]["$ref"],
            "#/components/schemas/Renamed-Output"
        );
        assert_eq!(
            schemas["Holder-Input"]["properties"]["inner"]["$ref"],
            "#/components/schemas/Renamed-Input"
        );
        assert_eq!(
            request_ref(&spec, "/holder"),
            "#/components/schemas/Holder-Input"
        );
        assert_eq!(
            response_ref(&spec, "/holder"),
            "#/components/schemas/Holder-Output"
        );
    }

    /// Goal: a response-only type is documented by what is sent, under its plain name.
    /// Method: route the type only as a response, then check its field name and that no
    /// split variant exists.
    #[test]
    fn response_only_type_uses_serialize_contract() {
        let spec = spec(ApiRouter::new().api_route("/renamed", get(give_renamed)));
        let schemas = spec["components"]["schemas"].as_object().expect("schemas");
        assert!(schemas["Renamed"]["properties"].get("type").is_some());
        assert!(schemas.contains_key("Renamed-Input").not());
        assert!(schemas.contains_key("Renamed-Output").not());
    }

    /// Goal: path and query types, which aide inlines as parameters, leave no unused schema
    /// behind, while types reached only through another schema survive.
    /// Method: route path, query and body extractors, then check the schema names left and
    /// that the parameters stay inlined.
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

    /// Takes the name the split would give `Renamed`'s request variant.
    #[derive(Deserialize, JsonSchema)]
    #[schemars(rename = "Renamed-Input")]
    struct TakesSplitName {
        #[expect(dead_code, reason = "only the schema is read")]
        value: u8,
    }

    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_renamed_input(Json(_): Json<TakesSplitName>) {}

    /// Goal: a split whose variant name is already taken fails loudly rather than overwriting
    /// a schema.
    /// Method: route a split type both ways and a separate type named after its request
    /// variant, then expect the merge to panic.
    #[test]
    #[should_panic(expected = "Renamed-Input already exists")]
    fn split_name_collision_panics() {
        spec(
            ApiRouter::new()
                .api_route("/renamed", post(take_renamed))
                .api_route("/renamed", get(give_renamed))
                .api_route("/renamed-input", post(take_renamed_input)),
        );
    }

    mod first {
        /// Shares its name with `second::Twin`.
        #[derive(serde::Deserialize, schemars::JsonSchema)]
        pub struct Twin {
            #[expect(dead_code, reason = "only the schema is read")]
            pub value: u8,
        }
    }

    mod second {
        /// Shares its name with `first::Twin`.
        #[derive(serde::Deserialize, schemars::JsonSchema)]
        pub struct Twin {
            #[expect(dead_code, reason = "only the schema is read")]
            pub flag: bool,
        }
    }

    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_first_twin(Json(_): Json<first::Twin>) {}
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_second_twin(Json(_): Json<second::Twin>) {}

    /// Goal: two types that share a name fail the build of the spec rather than reaching
    /// clients under a numbered name that depends on route order.
    /// Method: route two same-named types as request bodies, then expect the merge to panic
    /// on the name schemars numbered.
    #[test]
    #[should_panic(expected = "Twin2 collides with Twin")]
    fn numbered_name_collision_panics() {
        spec(
            ApiRouter::new()
                .api_route("/first", post(take_first_twin))
                .api_route("/second", post(take_second_twin)),
        );
    }

    /// Every place outside `components.schemas` that can hold a schema.
    const SCHEMA_SITES: [&str; 13] = [
        "/paths/~1a/parameters/0/schema",
        "/paths/~1a/get/responses/200/headers/X-H/schema",
        "/paths/~1a/get/responses/200/content/application~1json/schema",
        "/paths/~1a/get/responses/200/content/application~1json/encoding/part/headers/X-E/schema",
        "/paths/~1a/get/callbacks/cb/{$url}/parameters/0/schema",
        "/webhooks/w/parameters/0/schema",
        "/components/responses/r/content/application~1json/schema",
        "/components/parameters/p/schema",
        "/components/requestBodies/b/content/application~1json/schema",
        "/components/headers/h/schema",
        "/components/headers/hc/content/application~1json/schema",
        "/components/callbacks/c/{$url}/parameters/0/schema",
        "/components/pathItems/i/parameters/0/schema",
    ];

    /// A document with an empty schema at every one of `SCHEMA_SITES`.
    fn document_with_every_site() -> Value {
        let parameter =
            |name: &str| serde_json::json!({"name": name, "in": "header", "schema": {}});
        let json_body = serde_json::json!({"application/json": {"schema": {}}});
        serde_json::json!({
            "paths": {"/a": {
                "parameters": [parameter("p")],
                "get": {
                    "responses": {"200": {
                        "description": "",
                        "headers": {"X-H": {"schema": {}}},
                        "content": {"application/json": {
                            "schema": {},
                            "encoding": {"part": {"headers": {"X-E": {"schema": {}}}}},
                        }},
                    }},
                    "callbacks": {"cb": {"{$url}": {"parameters": [parameter("c")]}}},
                },
            }},
            "webhooks": {"w": {"parameters": [parameter("w")]}},
            "components": {
                "responses": {"r": {"description": "", "content": json_body}},
                "parameters": {"p": parameter("p")},
                "requestBodies": {"b": {"content": json_body}},
                "headers": {"h": {"schema": {}}, "hc": {"content": json_body}},
                "callbacks": {"c": {"{$url}": {"parameters": [parameter("c")]}}},
                "pathItems": {"i": {"parameters": [parameter("i")]}},
            },
        })
    }

    /// Goal: a response schema anywhere in the document gets its final ref and keeps its
    /// definition, not just in operation bodies and parameters.
    /// Method: for each site, place only there a ref to a response-direction type, finish the
    /// document, then check the ref and the definition. The document round trip proves each
    /// site parses into aide's types rather than vanishing.
    #[test]
    fn refs_at_every_schema_site_are_merged_and_kept() {
        for site in SCHEMA_SITES {
            let mut document = document_with_every_site();
            let (schema, _) = serialize_subschema_for::<Same>();
            *document.pointer_mut(site).expect("the site exists") = schema.to_value();
            // `OpenApi` itself borrows its version string, so it parses only part by part.
            let part = |key: &str| document[key].clone();
            let mut api = OpenApi {
                paths: serde_json::from_value(part("paths")).expect("paths parse"),
                webhooks: serde_json::from_value(part("webhooks")).expect("webhooks parse"),
                components: serde_json::from_value(part("components")).expect("components parse"),
                ..OpenApi::default()
            };
            finish_spec(&mut api);
            let spec = serde_json::to_value(api).expect("the spec serializes");
            assert_eq!(
                spec.pointer(site).expect("the site survives")["$ref"],
                "#/components/schemas/Same",
                "{site}"
            );
            assert!(
                spec["components"]["schemas"].get("Same").is_some(),
                "{site} keeps its definition"
            );
        }
    }

    /// Holds tuples, like `Profile::speed_profile` and `LightingSettings::colors`.
    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Tuples {
        points: Vec<(f64, u8)>,
        color: (u8, u8, u8),
    }

    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn take_tuples(Json(_): Json<Tuples>) {}
    #[allow(clippy::unused_async, reason = "axum handlers must be async")]
    async fn give_tuples() -> Json<Tuples> {
        Json(Tuples {
            points: Vec::new(),
            color: (0, 0, 0),
        })
    }

    /// Goal: tuples are valid JSON Schema 2020-12, which `OpenAPI` 3.1 uses. Generated
    /// clients reject the draft-07 `items: [..]` form.
    /// Method: route a tuple-holding type both ways, then check its tuple fields.
    #[test]
    fn tuples_use_prefix_items() {
        let spec = spec(
            ApiRouter::new()
                .api_route("/tuples", post(take_tuples))
                .api_route("/tuples", get(give_tuples)),
        );
        let properties = &spec["components"]["schemas"]["Tuples"]["properties"];
        let points = &properties["points"]["items"];
        assert_eq!(points["prefixItems"][0]["type"], "number");
        assert_eq!(points["prefixItems"][1]["type"], "integer");
        assert!(points.get("items").is_none());
        let color = &properties["color"];
        assert_eq!(color["prefixItems"].as_array().map(Vec::len), Some(3));
        assert!(color.get("items").is_none());
    }

    /// Goal: only subschemas change. Instance data may hold an `items` array, and a property
    /// may be named after a data keyword.
    /// Method: restore a hand-built schema with both, plus a draft-07 tail schema.
    #[test]
    fn restore_prefix_items_skips_data_and_moves_tail() {
        let mut schema = serde_json::json!({
            "default": {"items": [1, 2]},
            "properties": {
                "default": {"items": [{"type": "string"}], "additionalItems": {"type": "integer"}},
            },
        });
        let object = schema.as_object_mut().expect("an object");
        restore_prefix_items(object);
        assert_eq!(schema["default"], serde_json::json!({"items": [1, 2]}));
        let field = &schema["properties"]["default"];
        assert_eq!(
            field["prefixItems"],
            serde_json::json!([{"type": "string"}])
        );
        assert_eq!(field["items"], serde_json::json!({"type": "integer"}));
        assert!(field.get("additionalItems").is_none());
    }
}
