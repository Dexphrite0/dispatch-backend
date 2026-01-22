use actix_web::{web, App, HttpServer, HttpResponse, middleware};
use mongodb::{Client, bson::doc};
use mongodb::options::ClientOptions;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    firstName: String,
    lastName: String,
    email: String,
    password: String,
    role: Option<String>,
}

#[derive(Deserialize)]
struct SignupRequest {
    firstName: String,
    lastName: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    message: String,
    user_id: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

async fn signup(
    req: web::Json<SignupRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");
    
    let hashed = bcrypt::hash(&req.password, 4).unwrap_or_else(|_| req.password.clone());
    
    let user = User {
        id: None,
        firstName: req.firstName.clone(),
        lastName: req.lastName.clone(),
        email: req.email.clone(),
        password: hashed,
        role: None,
    };

    match users.insert_one(&user, None).await {
        Ok(result) => {
            let user_id = result.inserted_id.to_string();
            HttpResponse::Ok().json(AuthResponse {
                message: "User created".to_string(),
                user_id,
            })
        }
        Err(_) => {
            HttpResponse::BadRequest().json(ErrorResponse {
                error: "Email already exists".to_string(),
            })
        }
    }
}

async fn login(
    req: web::Json<LoginRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");

    match users.find_one(doc! { "email": &req.email }, None).await {
        Ok(Some(user)) => {
            if bcrypt::verify(&req.password, &user.password).unwrap_or(false) {
                HttpResponse::Ok().json(AuthResponse {
                    message: "Login successful".to_string(),
                    user_id: user.id.unwrap_or_default(),
                })
            } else {
                HttpResponse::Unauthorized().json(ErrorResponse {
                    error: "Invalid password".to_string(),
                })
            }
        }
        _ => {
            HttpResponse::Unauthorized().json(ErrorResponse {
                error: "User not found".to_string(),
            })
        }
    }
}

#[derive(Deserialize)]
struct SetRoleRequest {
    user_id: String,
    role: String,
}

async fn set_role(
    req: web::Json<SetRoleRequest>,
    db: web::Data<mongodb::Database>,
) -> HttpResponse {
    let users = db.collection::<User>("users");

    match users.update_one(
        doc! { "_id": &req.user_id },
        doc! { "$set": { "role": &req.role } },
        None
    ).await {
        Ok(_) => {
            HttpResponse::Ok().json(serde_json::json!({
                "message": format!("Role set to {}", req.role)
            }))
        }
        Err(_) => {
            HttpResponse::BadRequest().json(ErrorResponse {
                error: "Failed to set role".to_string(),
            })
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    
    let mongo_url = std::env::var("MONGO_URL")
        .expect("MONGO_URL environment variable not set");
    
    let client_options = ClientOptions::parse(&mongo_url)
        .await
        .expect("Failed to parse MongoDB connection string");
    
    let client = Client::with_options(client_options)
        .expect("Failed to create MongoDB client");
    
    let db = client.database("my_app");
    let db = web::Data::new(db);

    println!("Starting server on http://0.0.0.0:8000");

    HttpServer::new(move || {
        App::new()
            .app_data(db.clone())
            .wrap(middleware::NormalizePath::trim())
            .route("/api/signup", web::post().to(signup))
            .route("/api/login", web::post().to(login))
            .route("/api/set-role", web::post().to(set_role))
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}