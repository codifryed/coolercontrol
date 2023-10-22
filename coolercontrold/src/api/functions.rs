use std::sync::Arc;

use actix_web::{get, HttpResponse, post, Responder};
use actix_web::web::{Data, Json};
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::ErrorResponse;
use crate::config::Config;
use crate::setting::Function;

/// Retrieves the persisted Function list
#[get("/functions")]
async fn get_functions(
    config: Data<Arc<Config>>
) -> impl Responder {
    match config.get_functions().await {
        Ok(functions) => HttpResponse::Ok().json(Json(FunctionsDto { functions })),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Set the given functions, overwriting any existing
#[post("/functions")]
async fn save_functions(
    functions_dto: Json<FunctionsDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    config.set_functions(&functions_dto.functions).await;
    match config.save_config_file().await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionsDto {
    functions: Vec<Function>,
}
