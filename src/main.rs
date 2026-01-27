use actix_web::{web, App, HttpServer, HttpResponse, middleware};
use actix_cors::Cors;
use mongodb::{Client, bson::{doc, oid::ObjectId}};
use mongodb::options::ClientOptions;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
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
struct LoginResponse {
    message: String,
    user_id: String,
    firstName: String,
    lastName: String,
    email: String,
    role: Option<String>,
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
    
    // FIRST: Check if email already exists
    match users.find_one(doc! { "email": &req.email }, None).await {
        Ok(Some(_)) => {
            // Email already exists - STOP here and return error
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "Email already exists".to_string(),
            });
        }
        Err(e) => {
            // Database error during check
            println!("Database error checking email: {}", e);
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: "Database error".to_string(),
            });
        }
        Ok(None) => {
            // Email is unique, continue with signup
        }
    }
    
    // SECOND: Hash password
    let hashed = bcrypt::hash(&req.password, 4).unwrap_or_else(|_| req.password.clone());
    
    // THIRD: Create user object
    let user = User {
        id: None,
        firstName: req.firstName.clone(),
        lastName: req.lastName.clone(),
        email: req.email.clone(),
        password: hashed,
        role: None,
    };

    // FOURTH: Insert user
    match users.insert_one(&user, None).await {
        Ok(result) => {
            // Get the inserted ID and convert to string
            let user_id = result.inserted_id.as_object_id()
                .map(|oid| oid.to_hex())
                .unwrap_or_else(|| "unknown".to_string());
            
            println!("User created successfully: {} with ID: {}", req.email, user_id);
            
            HttpResponse::Ok().json(AuthResponse {
                message: "User created".to_string(),
                user_id,
            })
        }
        Err(e) => {
            println!("Error inserting user: {}", e);
            HttpResponse::BadRequest().json(ErrorResponse {
                error: "Failed to create user".to_string(),
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
                let user_id = user.id
                    .map(|oid| oid.to_hex())
                    .unwrap_or_else(|| "unknown".to_string());
                
                println!("Login successful for: {}", req.email);
                println!("User role: {:?}", user.role);
                
                HttpResponse::Ok().json(LoginResponse {
                    message: "Login successful".to_string(),
                    user_id,
                    firstName: user.firstName,
                    lastName: user.lastName,
                    email: user.email,
                    role: user.role,
                })
            } else {
                println!("Invalid password for: {}", req.email);
                HttpResponse::Unauthorized().json(ErrorResponse {
                    error: "Invalid password".to_string(),
                })
            }
        }
        Ok(None) => {
            println!("User not found: {}", req.email);
            HttpResponse::Unauthorized().json(ErrorResponse {
                error: "User not found".to_string(),
            })
        }
        Err(e) => {
            println!("Database error during login: {}", e);
            HttpResponse::InternalServerError().json(ErrorResponse {
                error: "Database error".to_string(),
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

    // Convert the user_id string back to ObjectId
    let user_oid = match ObjectId::parse_str(&req.user_id) {
        Ok(oid) => oid,
        Err(e) => {
            println!("Invalid user_id format: {} - Error: {}", req.user_id, e);
            return HttpResponse::BadRequest().json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
            });
        }
    };

    match users.update_one(
        doc! { "_id": user_oid },
        doc! { "$set": { "role": &req.role } },
        None
    ).await {
        Ok(result) => {
            if result.modified_count > 0 {
                println!("Role set successfully for user: {} to {}", req.user_id, req.role);
                HttpResponse::Ok().json(serde_json::json!({
                    "message": format!("Role set to {}", req.role)
                }))
            } else {
                println!("User not found for role update: {}", req.user_id);
                HttpResponse::BadRequest().json(ErrorResponse {
                    error: "User not found".to_string(),
                })
            }
        }
        Err(e) => {
            println!("Error setting role: {}", e);
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
    let users = db.collection::<User>("users");
    
    // Create unique index on email - prevents duplicate emails at database level
    match users.create_index(
        mongodb::IndexModel::builder()
            .keys(doc! { "email": 1 })
            .options(mongodb::options::IndexOptions::builder().unique(true).build())
            .build(),
        None,
    )
    .await {
        Ok(_) => println!("✓ Email unique index created/verified"),
        Err(e) => println!("Warning: Could not create index: {}", e),
    }
    
    let db = web::Data::new(db);

    println!("Starting server on http://0.0.0.0:8000");

    HttpServer::new(move || {
        // Proper CORS configuration
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);

        App::new()
            .app_data(db.clone())
            .wrap(cors)
            .wrap(middleware::NormalizePath::trim())
            .route("/api/signup", web::post().to(signup))
            .route("/api/login", web::post().to(login))
            .route("/api/set-role", web::post().to(set_role))
    })
    .bind("0.0.0.0:8000")?
    .run()
    .await
}
